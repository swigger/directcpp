mod code_match;
mod mangle;
mod parse;
mod util;
mod tests;

use parse::{Functions, map_to_cxx};
use std::collections::HashMap;
use proc_macro2::{TokenStream, Span};
use proc_macro::{TokenStream as TS0, TokenTree};
use std::env;
use regex::Regex;
use syn;
use std::str::FromStr;
use std::sync::Mutex;
use crate::mangle::*;
use crate::util::*;


const TYPE_POD:i32 = 0;
const TYPE_DTOR_TRIVIAL_MOVE:i32 = 1;  // 假定所有类型默认都是trivial move, non-trivial dtor

lazy_static::lazy_static! {
	static ref TYPE_STRATEGY: Mutex<HashMap<String, i32>> = Mutex::new(HashMap::new());
}

#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct FFIBuilder{
	is_cpp: bool,
	extc_code: String,
	norm_code: String,
	err_str: String,
	asm_used: bool
}

impl FFIBuilder {
	pub fn new() -> Self { Self::default() }

	fn dtor_code(tp: &str) -> String {
		let tp = map_to_cxx(tp);
		let dtor_name = dtor_name(tp);
		let tp1 = tp.replace("<", "_").replace(">", "_");
		format!("\t#[link_name = \"{dtor_name}\"]\n\tfn ffi__free_{tp1}(__o: *mut usize);\n")
	}
	fn sp_dtor_code(tp: &str) -> String {
		let dtor_name = sp_dtor_name(tp);
		format!("\t#[link_name = \"{dtor_name}\"]\n\tfn ffi__freeSP_{tp}(__o: *mut usize);\n")
	}

	fn show_dtor(self: &mut Self, tp: &str, rtwrap:&str, tp_cpp: &str)->Result<(), &str> {
		let tp_strategy = match rtwrap {
			"POD" => TYPE_POD,
			// type can be dtor, and can be trivial move. trivial move is required for Rust.
			"SharedPtr"|"UniquePtr"|"Vec"|"" => TYPE_DTOR_TRIVIAL_MOVE,
			_ => {
				self.err_str = format!("type {} not supported", rtwrap);
				return Err(&self.err_str);
			}
		};
		let mut mp = TYPE_STRATEGY.lock().unwrap();
		let mut tp1 = if let Some(x) = mp.get(tp) {
			if *x >> 16 != tp_strategy {
				self.err_str = format!("type {tp} strategy conflict");
				return Err(&self.err_str);
			}
			*x
		} else {
			tp_strategy << 16
		};

		if tp_strategy != TYPE_POD {
			if (tp1 & 1 == 0) && rtwrap != "SharedPtr" {
				tp1 |= 1;
				self.extc_code += &Self::dtor_code(tp_cpp);
			}
			if rtwrap == "UniquePtr" && tp1 & 2 == 0 {
				tp1 |= 2;
				self.norm_code += &format!("
impl ManDtor for {tp} {{
	unsafe fn __dtor(ptr: *mut [u8;0]) {{
		if ptr as usize != 0 {{
			ffi__free_{tp}(ptr as *mut usize);
		}}
	}}
}}");
			}
			if rtwrap == "SharedPtr" && tp1 & 4 == 0 {
				tp1 |= 4;
				self.extc_code += &Self::sp_dtor_code(tp);
				self.norm_code += &format!("
impl DropSP for {tp} {{
	unsafe fn __drop_sp(ptr: *mut [u8;0]) {{
		if ptr as usize != 0 {{
			ffi__freeSP_{tp}(ptr as *mut usize);
		}}
	}}
}}\n");
			}
		}
		mp.insert(tp.to_string(), tp1);
		Ok(())
	}

	fn get_link_name(self: &Self, func: &SimpFunc, is_cpp: bool)
		-> Result<String, &'static str>
	{
		if ! is_cpp {
			Ok(func.fn_name.to_string())
		} else {
			mangle(&func)
		}
	}

	fn build_one_func(self:&mut Self, func: &SimpFunc, is_cpp: bool) -> Result<(), &str>{
		let mut args_c = Vec::new();
		let mut args_r = Vec::new();
		let mut args_usage = Vec::new();
		let mut fn_name =  if let Some(pos) = func.fn_name.rfind("::") {
			func.fn_name[pos + 2..].to_string()
		}else{
			func.fn_name.to_string()
		};
		if !func.klsname.is_empty() {
			args_c.push("this__: *const u8".to_string());
			args_r.push(format!("this__: CPtr<{}>", &func.klsname));
			args_usage.push("this__.addr as *const u8".to_string());
			fn_name = format!("{}__{}", &func.klsname, &func.fn_name);
		}
		let return_code_r = if func.is_async {
			if func.ret.tp.is_empty() {
				return Err("async function must have a return type");
			}
			if ! func.ret.tp_wrap.is_empty() {
				self.err_str = format!("currently async function should only return non-generic types. wrap your type into a struct if need. unsupported return type {}", &func.ret.raw_str);
				return Err(&self.err_str);
			}
			args_c.push("addr: usize".to_string());
			args_usage.push("dyn_fv_addr".to_string());
			format!(" -> {}", func.ret.tp_full)
		} else {
			match &func.ret.tp_wrap as &str {
				"" if func.ret.tp.is_empty() => String::new(),
				"POD" => format!(" -> {}", func.ret.tp),
				_ => format!(" -> {}", func.ret.tp_full),
			}
		};

		enum RetKind {
			RtPrimitive,
			RtCPtr,
			RtSharedPtr,
			RtObject,
		}
		let is_a64 = cfg!(target_arch="aarch64");
		let mut ret_indirect = String::new();
		let mut ret_kind = RetKind::RtPrimitive;
		let return_code_c = match &func.ret.tp_wrap as &str {
			"CPtr" => {
				ret_kind = RetKind::RtCPtr;
				" -> *const u8".to_string()
			},
			"SharedPtr"|"UniquePtr" => {
				ret_kind = RetKind::RtSharedPtr;
				if is_a64 {
					self.asm_used = true;
					ret_indirect = format!("let __rtox8 = &mut __rto as *mut {} as *mut u8;\n\t\t", &func.ret.tp_full);
				} else {
					args_c.push("__rto: * mut u8".to_string());
					args_usage.push(format!("&mut __rto as *mut {} as *mut u8", &func.ret.tp_full));
				}
				"".to_string()
			},
			"" if func.is_async => String::new(),
			"" if func.ret.tp.is_empty() => String::new(),
			"" if func.ret.is_primitive => format!(" -> {}", func.ret.tp),
			""|"POD"|"Vec" => {
				ret_kind = RetKind::RtObject;
				if is_a64 {
					self.asm_used = true;
					ret_indirect = "let __rtox8 = &mut __rta as *mut usize;\n\t\t".to_string();
				} else {
					args_c.push("__rto: * mut usize".to_string());
					args_usage.push("&mut __rta as *mut usize".to_string());
				}
				"".to_string()
			}
			_ => {
				self.err_str = format!("return type {} not supported", &func.ret.raw_str);
				return Err(&self.err_str);
			}
		};

		for arg in &func.arg_list {
			let mut args_x_done = false;
			let is_ref = arg.tp_full.chars().next().unwrap() == '&';
			match arg.tp_wrap.as_str() {
				""|"POD" if is_ref => {
					match arg.tp.as_str() {
						"CStr" => {
							args_x_done = true;
							args_c.push(format!("{}: *const i8", &arg.name));
							args_r.push(format!("{}: &CStr", &arg.name));
							args_usage.push(format!("{}.as_ptr()", &arg.name))
						},
						"str" => {
							args_x_done = true;
							args_c.push(format!("{}: *const u8, {}_len: usize", &arg.name, &arg.name));
							args_r.push(format!("{}: &str", &arg.name));
							args_usage.push(format!("{}.as_ptr()", &arg.name));
							args_usage.push(format!("{}.len()", &arg.name));
						},
						"[u8]" => {
							args_x_done = true;
							args_c.push(format!("{}: *const u8, {}_len: usize", &arg.name, &arg.name));
							args_r.push(format!("{}: &[u8]", &arg.name));
							args_usage.push(format!("{}.as_ptr()", &arg.name));
							args_usage.push(format!("{}.len()", &arg.name));
						},
						_ => args_usage.push(format!("{} as *{} {}", &arg.name, select_val(arg.is_const, "const", "mut"), &arg.tp)),
					}
				},
				"CPtr" => args_usage.push(format!("{}.addr as * const u8", &arg.name)),
				"Option" => match is_ref {
					true => args_usage.push(format!("{}.as_ref().map_or(0 as * const {}, |x| x as * const {})", &arg.name, &arg.tp, &arg.tp)),
					false => args_usage.push(format!("{}.map_or(0 as * const {}, |x| x as * const {})", &arg.name, &arg.tp, &arg.tp)),
				},
				// this is not tested, normally you should use CPtr<xx> for arguments, don't use these.
				"SharedPtr"|"UniquePtr" => args_usage.push(format!("{}.as_cptr().addr as * const {}", &arg.name, &arg.tp)),
				_ if arg.is_primitive => args_usage.push(format!("{}", &arg.name)),
				_ => {
					let suggested_str = arg.raw_str.replace(":", ": &");
					self.err_str = format!("function \"{}\" argument \"{}\" not supported, \
					you should always use a reference for non-primitive types in interop functions.\n\
					try use \"{}\" instead.", func.fn_name, &arg.raw_str, &suggested_str);
					return Err(&self.err_str);
				}
			};
			if !args_x_done {
				args_c.push(format!("{}: {}", &arg.name, &arg.tp_asc));
				args_r.push(format!("{}: {}", &arg.name, &arg.tp_full));
			}
		}

		let link_name = if func.is_async {
			let sa = SimpArg{
				name: "dyn_fv_addr".to_string(),
				tp: "usize".to_string(),
				tp_full: "usize".to_string(),
				tp_wrap: "".to_string(),
				tp_cpp: format!("ValuePromise<{}>*", func.ret.tp_cpp),
				is_const: false,
				is_primitive: true,
				raw_str: "usize".to_string(),
				tp_asc: "usize".to_string()
			};
			let mut func1 = func.clone();
			func1.ret.tp = "".to_string();
			func1.ret.tp_cpp = "".to_string();
			func1.ret.is_primitive = true;
			func1.arg_list.insert(0, sa);
			self.get_link_name(&func1, is_cpp)?
		} else {
			self.get_link_name(&func, is_cpp)?
		};
		let fnstart = format!("{} {}fn {}({}){}", &func.access,
			if func.is_async { "async " } else { "" },
			&fn_name, args_r.join(", "), return_code_r);
		self.extc_code += &format!("\t#[link_name = \"{link_name}\"]\n\tfn ffi__{fn_name}({}){};\n",
			args_c.join(", "), return_code_c);
		match ret_kind {
			RetKind::RtPrimitive => {},
			_ => {
				if let Err(s) = self.show_dtor(&func.ret.tp, &func.ret.tp_wrap, &func.ret.tp_cpp) {
					self.err_str = s.to_string();
					return Err(&self.err_str);
				}
			}
		}
		if ! ret_indirect.is_empty() {
			// \ asm!(\"mov x8, {{xval1}}\", xval1 = in(reg) __rtox8);
			//asm!(\"mov x8, {{x}}\", x = in(reg) __rtox8);
			if args_usage.len() > 0 {
				let idx = args_usage.len() - 1;
				let s0 = args_usage[idx].as_str();
				let s0 = format!("{{let __argk={}; asm!(\"mov x8, {{xval1}}\", xval1=in(reg) __rtox8); __argk}}", s0);
				args_usage[idx] = s0;
			} else {
				let s0 = "asm!(\"mov x8, {xval1}\", xval1=in(reg) __rtox8);\n\t\t";
				ret_indirect += s0;
			}
		}
		let usage = args_usage.join(", ");
		let norm_code = match ret_kind {
			RetKind::RtPrimitive if func.is_async => {
				format!("let mut arr : [usize;2] = [0, 0];\n\
					let mut fv= std::pin::pin!(FutureValue::<{}>::default());\n\
					let dyn_fv_addr = fv.copy_vtbl(&mut arr);\n\
					unsafe {{ ffi__{fn_name}({usage}); }}\n\
					fv.await", &func.ret.tp)
			},
			RetKind::RtPrimitive => format!("unsafe {{ ffi__{fn_name}({usage}) }}"),
			RetKind::RtCPtr => format!("CPtr{{ addr: unsafe {{ ffi__{fn_name}({usage}) as usize }}, _phantom: std::marker::PhantomData }}"),
			RetKind::RtSharedPtr => {
				let wrap1 = &func.ret.tp_wrap as &str;
				let ret_type = &func.ret.tp as &str;
				format!("let mut __rto = {wrap1}::<{ret_type}>::default();\n\
					\tunsafe {{ {ret_indirect} ffi__{fn_name}({usage}); }}\n\
					\t__rto")
			},
			RetKind::RtObject => {
				let mut ret_type = &func.ret.tp_full as &str;
				let tp1 = func.ret.tp_cpp.replace("<", "_").replace(">", "_");
				let call_free = match func.ret.tp_wrap.as_str() {
					"POD" => {
						ret_type = &func.ret.tp as &str;
						"".to_string()
					},  // no destructor for POD
					_ => format!("ffi__free_{}(&mut __rta as *mut usize);\n\t\t", &tp1),
				};
				format!("const SZ:usize = (std::mem::size_of::<{ret_type}>()+16)/8;\n\
					\tlet mut __rta : [usize;SZ] = [0;SZ];\n\
					\tunsafe {{ {ret_indirect}\n\
					\t\tffi__{fn_name}({usage}); \n\
					\t\tlet __rto = (*(&__rta as *const usize as *const {ret_type})).clone();\n\
					\t\t{call_free}__rto\n\
					\t}}")
			},
			// _ => return Err("xx")
		};
		self.norm_code += &format!("{fnstart} {{\n\t{norm_code}\n}}\n");
		Ok(())
	}

	pub fn build_bridge_code(self: &mut Self, input: TokenStream) -> Result<TokenStream, &str> {
		let mut xxx = Functions::new();
		if let Err(s) = xxx.parse_ts(input) {
			self.err_str = s.to_string();
			return Err(&self.err_str);
		}

		for func in &xxx.funcs {
			if let Err(_) = self.build_one_func(func, xxx.is_cpp) {
				return Err(&self.err_str);
			}
		}
		let extc_code = move_obj(&mut self.extc_code);
		let norm_code = move_obj(&mut self.norm_code);
		let use_asm = select_val(self.asm_used, "use std::arch::asm;\n", "");
		let all_code = format!("{use_asm}extern \"C\" {{\n{extc_code}}}\n{norm_code}\n");
		if env_as_bool("RUST_BRIDGE_DEBUG") {
			println!("{}", all_code);
		}
		TokenStream::from_str(&all_code).map_err(|e| {
			self.err_str = e.to_string();
			self.err_str.as_str()
		})
	}
}

extern "C" {
	fn enable_msvc_debug_c();
}

#[proc_macro_attribute]
pub fn bridge(_args: TS0, input: TS0) -> TS0 {
	let mut bb = FFIBuilder::new();
	match bb.build_bridge_code(input.into()) {
		Ok(code) => code.into(),
		Err(e) => syn::Error::new(Span::call_site(), e).to_compile_error().into()
	}
}

#[proc_macro_attribute]
pub fn enable_msvc_debug(args: TS0, _input: TS0) -> TS0
{
	let enable_ = if let Some(TokenTree::Ident(val)) = args.into_iter().next() {
		val.to_string().parse::<i32>().unwrap_or(-1)
	} else {
		-1
	};
	let is_debug = match enable_ {
		0 => false,
		1 => true,
		_ => {
			match env::var("OUT_DIR") {
				Ok(profile) => Regex::new(r"[\\/]target[\\/]debug[\\/]").unwrap().is_match(&profile),
				Err(_) => false,
			}
		}
	};
	if is_debug {
		unsafe{ enable_msvc_debug_c(); }
	}
	TS0::new()
}

use proc_macro2::TokenStream;
use crate::code_match;
use crate::util::*;
use crate::mangle::*;

pub fn map_to_cxx(tp: &str) -> &str {
	match tp {
		"String" => "RustString",
		_ => tp
	}
}

pub struct Functions {
	pub funcs: Vec<SimpFunc>,
	pub is_cpp: bool,
	cmo: code_match::CodeMatchObject,
	err_str: String,
}

impl Functions {
	pub fn new() -> Self { Self{
		funcs: Vec::new(),
		is_cpp: false,
		cmo: code_match::CodeMatchObject::new(),
		err_str: "".to_string(),
	} }

	fn parse_attr(self: &mut Self, ts: TokenStream, ns:&mut String) -> String {
		let mut dest = self.cmo.build_string(ts, true);
		{
			// Let's search all class hints.
			let reg = self.cmo.build_regex("`([ class struct `]) ( `(`iden`) )", true).unwrap();
			let mut vr = Vec::new();
			let mut dest1 = dest.clone();
			while code_match::test_match(&mut dest1, &reg, &mut vr) {
				let klsname = self.cmo.back_str(&vr[1]);
				let ktype = self.cmo.back_str(&vr[0]);
				let _ = set_class_hint(&klsname, match ktype.as_str() {
					"class" => ClassHint::StrongClass,
					_ => ClassHint::StrongStruct,
				});
			}
		}
		{
			let reg = self.cmo.build_regex("namespace ( `(.*?`) )", true).unwrap();
			let mut vr = Vec::new();
			let mut dest1 = dest.clone();
			if code_match::test_match(&mut dest1, &reg, &mut vr) {
				*ns = self.cmo.back_str(&vr[0]);
			}
		}

		let reg = self.cmo.build_regex(" member_of ( `(`iden`) )", true).unwrap();
		let mut vr = Vec::new();
		if code_match::test_match(&mut dest, &reg, &mut vr) {
			let klsname = self.cmo.back_str(&vr[0]);
			let _ = set_class_hint(&klsname, ClassHint::StrongClass);
			return klsname;
		}
		"".to_string()
	}
	fn is_compatible_rettype(x: &str) -> bool {
		if x.is_empty() { return true; }
		if x == "i8" || x == "i16" || x == "i32" || x == "i64" { return true; }
		if x == "u8" || x == "u16" || x == "u32" || x == "u64" { return true; }
		if x == "f32" || x == "f64" { return true; }
		if x == "bool" { return true; }
		false
	}

	fn build_as_c_arg(self: &mut Self, arg: &mut SimpArg) -> Result<(), ()> {
		if arg.tp_full.is_empty() {
			arg.is_primitive = true;
			// arg.tp_cpp = "void".to_string();
			return Ok(());
		}
		let is_ref = arg.tp_full.chars().next().unwrap() == '&';
		arg.tp_asc = match arg.tp_wrap.as_str() {
			"Option" => format!("*{} {}", if arg.is_const {"const"} else {"mut"},  &arg.tp),
			"CPtr" => String::from("*const u8"),
			"SharedPtr"|"UniquePtr" => {
				let _ = set_class_hint(&arg.tp, ClassHint::WeakClass);
				String::from("*const u8")
			},
			_ if is_ref => format!("*{} {}", if arg.is_const {"const"} else {"mut"},  &arg.tp),
			_ => arg.tp_full.clone(),
		};

		arg.is_primitive = true;
		let cpp_type = match &arg.tp as &str {
			"i8" => "int8_t",
			"i16" => "int16_t",
			"i32" => "int",
			"i64" => "int64_t",
			"u8" => "uint8_t",
			"u16" => "uint16_t",
			"u32" => "uint32_t",
			"u64" => "uint64_t",
			"f32" => "float",
			"f64" => "double",
			"bool" => "bool",
			"String" => {
				arg.is_primitive=false;
				"RustString"   //special map for String
			},
			_ => {arg.is_primitive = false; &arg.tp as &str},
		};
		// arg.name="" means it's a return value.
		arg.tp_cpp = match arg.tp_wrap.as_str() {
			"CPtr" => format!("{}*", cpp_type),
			"SharedPtr" => match arg.name.as_str() {
				"" =>  format!("shared_ptr<{}>", cpp_type),
				_ => format!("{}*", cpp_type),
			}
			"UniquePtr" => match arg.name.as_str() {
				"" =>  format!("unique_ptr<{}>", cpp_type),
				_ => format!("{}*", cpp_type),
			}
			"Option" => match arg.is_const {
				true=> format!("const {}*", cpp_type),
				false => format!("{}*", cpp_type),
			}
			"Vec" => {
				arg.is_primitive = false;
				if is_ref {
					format!("{}RustVec<{}>&", select_val(arg.is_const, "const ", ""), cpp_type)
				} else {
					format!("RustVec<{}>", cpp_type)
				}
			}
			""|"POD" if is_ref => {
				match arg.tp.as_str() {
					"CStr" => "const char*".to_string(),
					"str" => "const char*,size_t".to_string(),
					"[u8]" => "const uint8_t*,size_t".to_string(),
					_ => {
						let _ = set_class_hint(&arg.tp, ClassHint::WeakStruct);
						format!("{}{}&", select_val(arg.is_const, "const ", ""), cpp_type)
					}
				}
			}
			"" if arg.is_primitive => cpp_type.to_string(),
			"" if arg.name=="" => { //a return value.
				let _ = set_class_hint(&arg.tp, ClassHint::WeakStruct);
				cpp_type.to_string()
			}
			_ => {
				self.err_str = format!("unkown type {}", arg.tp_full);
				return Err(());
			}
		};
		Ok(())
	}

	fn parse_one_func(self: &mut Self, curfunc: &mut SimpFunc, ts: TokenStream) -> Result<(), ()>
	{
		let mut dest = self.cmo.build_string(ts, true);
		let reg0 = [ "`(`iden`) : `( & `)? `( mut `)?  `(`iden`) `(?: , `|$)",
			"`(`iden`) : &`? CPtr< &`? `(`iden`) >  `(?: , `|$)",
			"`(`iden`) : Option< & `(`iden`) >  `(?: , `|$)",
			"`(`iden`) : & `(`iden`) < `(`iden`) >  `(?: , `|$)",
			"`(`iden`) : & `( [u8] `)  `(?: , `|$)",
		];
		let reg = reg0.map(|x| self.cmo.build_regex(x, true).unwrap());
		let mut vr = Vec::new();
		let mut arg_idx = 1;
		loop {
			let dest_cp = dest.clone();
			let idx = code_match::test_match2(&mut dest, &reg, &mut vr);
			if idx < 0 {
				break;
			}
			let mut cur_arg = SimpArg::default();
			let raw_len = dest_cp.len() - dest.len();
			cur_arg.raw_str = self.cmo.back_str(&dest_cp[0..raw_len]);
			cur_arg.name = self.cmo.back_str(&vr[0]);
			if idx == 0 {
				cur_arg.is_const = vr[2].is_empty();
				cur_arg.tp = self.cmo.back_str(&vr[3]);
				cur_arg.tp_full = if vr[1].is_empty() { "".to_string() } else { "&".to_string() };
				if ! cur_arg.is_const {
					cur_arg.tp_full += "mut ";
				}
				cur_arg.tp_full += &cur_arg.tp;
			} else if idx == 2 {
				cur_arg.tp = self.cmo.back_str(&vr[2]);
				cur_arg.tp_wrap = "Option".to_string();
				cur_arg.tp_full = format!("Option<&{}>", &cur_arg.tp);
			} else if idx == 3 {
				cur_arg.is_const = true;
				cur_arg.tp = self.cmo.back_str(&vr[2]);
				cur_arg.tp_wrap = self.cmo.back_str(&vr[1]);
				cur_arg.tp_full = format!("&{}<{}>", &cur_arg.tp_wrap, &cur_arg.tp);
			} else if idx == 1 {
				cur_arg.tp = self.cmo.back_str(&vr[1]);
				cur_arg.tp_wrap = "CPtr".to_string();
				cur_arg.tp_full = format!("CPtr<{}>", &cur_arg.tp);
			} else if idx == 4 {
				cur_arg.tp = self.cmo.back_str(&vr[1]);
				cur_arg.is_const = true;
				cur_arg.tp_full = "&[u8]".to_string();
			} else {
				self.err_str = format!("argument {} not supported", arg_idx);
				return Err(());
			}
			self.build_as_c_arg(&mut cur_arg)?;
			curfunc.arg_list.push(cur_arg);
			arg_idx += 1;
		}
		if ! dest.is_empty() {
			self.err_str = format!("argument {arg_idx} not supported");
			return Err(());
		}
		Ok(())
	}

	pub fn parse_ts(self: &mut Self, input: TokenStream) -> Result<(), &str> {
		let mut dest = self.cmo.build_string(input, false);
		let reg0 = self.cmo.build_regex(" extern `([ \"C\" \"C++\" `]) {} `(\\d+) ", false).unwrap();
		let mut vr = Vec::new();
		if ! code_match::test_match(&mut dest, &reg0, &mut vr) {
			return Err("macro expect extern \"C\" {...}");
		}
		self.is_cpp = self.cmo.find_lit(vr[0].chars().nth(0).unwrap_or('\0'))
			.map(|x| x == "\"C++\"").unwrap_or(false);
		let ts1 = self.cmo.find_ts(&vr[1]).unwrap();
		let mut dest = self.cmo.build_string(ts1, false);
		let reg1 = self.cmo.build_regex("
		`(?:  #[] `(\\d+) `)?
		`( pub `| pub(crate) `\\d+)? `( async `)?
		fn   `(`iden`)   ()`(\\d+)
		`(?: -> `(?: `iden :: `)* `(`iden`| `iden < `iden > `) `)? ;", false).unwrap();

		let mut func_idx = 1;
		vr.clear();
		while code_match::test_match(&mut dest, &reg1, &mut vr) {
			let mut curfunc = SimpFunc::default();
			curfunc.access = self.cmo.back_str(&vr[1]);
			curfunc.is_async = !vr[2].is_empty();
			curfunc.fn_name = self.cmo.back_str(&vr[3]);
			if let Some(ts) = self.cmo.find_ts(&vr[0]) {
				let mut ns = String::new();
				curfunc.klsname = self.parse_attr(ts, &mut ns);
				if ns.len() > 0 {
					if curfunc.klsname.len() == 0 {
						curfunc.fn_name = format!("{}::{}", ns, curfunc.fn_name);
					} else {
						curfunc.klsname = format!("{}::{}", ns, curfunc.klsname);
					}
				}
			}
			let ts2 = self.cmo.find_ts(&vr[4]).unwrap();
			let ret_vec = vr[5].chars().collect::<Vec<char>>();
			curfunc.ret.raw_str = self.cmo.back_str2(ret_vec.iter());
			curfunc.ret.tp_full = curfunc.ret.raw_str.clone();
			if ret_vec.len() == 1 {
				curfunc.ret.tp = self.cmo.find_iden(ret_vec[0]).map(|x| x.to_string()).unwrap_or("".to_string());
			} else if ret_vec.len() == 4 {
				curfunc.ret.tp = self.cmo.find_iden(ret_vec[2]).map(|x| x.to_string()).unwrap_or("".to_string());
				curfunc.ret.tp_wrap = self.cmo.find_iden(ret_vec[0]).map(|x| x.to_string()).unwrap_or("".to_string());
			}
			curfunc.ret.is_primitive = curfunc.ret.tp_wrap.is_empty() && Self::is_compatible_rettype(&curfunc.ret.tp);
			if let Err(_) = self.build_as_c_arg(&mut curfunc.ret) {
				return Err(&self.err_str);
			}
			if let Err(_) = self.parse_one_func(&mut curfunc, ts2) {
				let x = move_obj(&mut self.err_str);
				self.err_str = format!("function {} error: {x}", curfunc.fn_name);
				return Err(&self.err_str);
			}
			self.funcs.push(curfunc);
			func_idx += 1;
		}
		if ! dest.is_empty() {
			let mut vv = dest.chars().collect::<Vec<char>>();
			if vv.len() > 10 {
				vv.truncate(10);
			}
			let olds = self.cmo.back_str2(vv.iter());
			self.err_str = format!("macro expect pub fn name(ARGLIST) -> ret_type; found mismatch at function {func_idx} near {olds}");
			return Err(&self.err_str);
		}
		Ok(())
	}
}

use proc_macro2::TokenStream;
use syn::{
	Attribute, FnArg, ForeignItem, ItemForeignMod, GenericArgument, Pat, PathArguments,
	ReturnType, Type, Visibility,
};
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
	err_str: String,
}

/// Render a `syn::Type` back into a compact type string (no incidental spaces),
/// e.g. `&mut MagicIn`, `CPtr<CppStruct>`, `&Vec<&str>`, `[u8]`.
fn type_str(ty: &Type) -> String {
	match ty {
		Type::Reference(r) => {
			format!("&{}{}", if r.mutability.is_some() { "mut " } else { "" }, type_str(&r.elem))
		}
		Type::Path(p) => {
			let seg = match p.path.segments.last() {
				Some(s) => s,
				None => return String::new(),
			};
			let base = seg.ident.to_string();
			match &seg.arguments {
				PathArguments::AngleBracketed(a) => {
					let inner: Vec<String> = a.args.iter().filter_map(|g| match g {
						GenericArgument::Type(t) => Some(type_str(t)),
						_ => None,
					}).collect();
					format!("{}<{}>", base, inner.join(","))
				}
				_ => base,
			}
		}
		Type::Slice(s) => format!("[{}]", type_str(&s.elem)),
		Type::Ptr(p) => {
			format!("*{}{}", if p.mutability.is_some() { "mut " } else { "const " }, type_str(&p.elem))
		}
		other => quote::quote!(#other).to_string().replace(' ', ""),
	}
}

/// The "core" identifier of a type: peel references and take the last path segment.
fn core_ident(ty: &Type) -> String {
	match ty {
		Type::Reference(r) => core_ident(&r.elem),
		Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
		Type::Slice(_) => type_str(ty),
		other => type_str(other),
	}
}

fn pat_name(pat: &Pat) -> String {
	match pat {
		Pat::Ident(pi) => pi.ident.to_string(),
		other => quote::quote!(#other).to_string().replace(' ', ""),
	}
}

fn path_to_string(path: &syn::Path) -> String {
	path.segments.iter()
		.map(|s| s.ident.to_string())
		.collect::<Vec<_>>()
		.join("::")
}

impl Functions {
	pub fn new() -> Self { Self{
		funcs: Vec::new(),
		is_cpp: false,
		err_str: "".to_string(),
	} }

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
			"str" => {
				arg.is_primitive=false;
				"rust_refstr_t"   //&str maps to the rust_refstr_t fat-pointer struct
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
			""|"POD" if arg.name=="" => { //a return value.
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

	/// Walk a `syn::Type` into the SimpArg IR fields (tp / tp_wrap / tp_full / is_const).
	/// `name` is "" for return values.
	fn parse_arg_type(&mut self, name: &str, ty: &Type) -> Result<SimpArg, ()> {
		let mut arg = SimpArg::default();
		arg.name = name.to_string();
		let tp_full = type_str(ty);
		arg.raw_str = if name.is_empty() { tp_full.clone() } else { format!("{}:{}", name, tp_full) };

		// Peel a single outer reference for inspection.
		let (is_ref, is_mut, core): (bool, bool, &Type) = match ty {
			Type::Reference(r) => (true, r.mutability.is_some(), r.elem.as_ref()),
			other => (false, false, other),
		};

		match core {
			Type::Slice(_) => {
				arg.tp = type_str(core); // e.g. "[u8]"
				arg.tp_wrap = String::new();
				arg.is_const = !is_mut;
				arg.tp_full = tp_full;
			}
			Type::Path(p) => {
				let seg = match p.path.segments.last() {
					Some(s) => s,
					None => { self.err_str = "empty type path".to_string(); return Err(()); }
				};
				let outer = seg.ident.to_string();
				match &seg.arguments {
					PathArguments::AngleBracketed(a) => {
						let inner_ident = a.args.iter().find_map(|g| match g {
							GenericArgument::Type(t) => Some(core_ident(t)),
							_ => None,
						}).unwrap_or_default();
						match outer.as_str() {
							"CPtr" => {
								arg.tp = inner_ident;
								arg.tp_wrap = "CPtr".to_string();
								arg.is_const = false;
								arg.tp_full = format!("CPtr<{}>", &arg.tp);
							}
							"Option" => {
								arg.tp = inner_ident;
								arg.tp_wrap = "Option".to_string();
								arg.is_const = false;
								arg.tp_full = format!("Option<&{}>", &arg.tp);
							}
							_ => {
								// Vec / SharedPtr / UniquePtr / POD / other wrappers.
								arg.tp = inner_ident;
								arg.tp_wrap = outer;
								arg.is_const = if is_ref { !is_mut } else { false };
								arg.tp_full = tp_full;
							}
						}
					}
					_ => {
						// Plain type (possibly behind a reference).
						arg.tp = outer;
						arg.tp_wrap = String::new();
						arg.is_const = if is_ref { !is_mut } else { true };
						arg.tp_full = tp_full;
					}
				}
			}
			_ => {
				arg.tp = type_str(core);
				arg.tp_wrap = String::new();
				arg.is_const = if is_ref { !is_mut } else { true };
				arg.tp_full = tp_full;
			}
		}
		Ok(arg)
	}

	fn parse_attr(&mut self, attr: &Attribute, ns: &mut String, curfunc: &mut SimpFunc) {
		let name = match attr.path().segments.last() {
			Some(s) => s.ident.to_string(),
			None => return,
		};
		match name.as_str() {
			"namespace" => {
				if let Ok(path) = attr.parse_args::<syn::Path>() {
					*ns = path_to_string(&path);
				}
			}
			"member_of" => {
				if let Ok(path) = attr.parse_args::<syn::Path>() {
					let klsname = path_to_string(&path);
					let _ = set_class_hint(&klsname, ClassHint::StrongClass);
					curfunc.klsname = klsname;
				}
			}
			"class" => {
				if let Ok(path) = attr.parse_args::<syn::Path>() {
					let _ = set_class_hint(&path_to_string(&path), ClassHint::StrongClass);
				}
			}
			"struct" => {
				if let Ok(path) = attr.parse_args::<syn::Path>() {
					let _ = set_class_hint(&path_to_string(&path), ClassHint::StrongStruct);
				}
			}
			_ => {}
		}
	}

	fn parse_ret(&mut self, output: &ReturnType) -> Result<SimpArg, ()> {
		let mut ret = match output {
			ReturnType::Default => SimpArg::default(),
			ReturnType::Type(_, ty) => self.parse_arg_type("", ty)?,
		};
		ret.is_primitive = ret.tp_wrap.is_empty() && Self::is_compatible_rettype(&ret.tp);
		self.build_as_c_arg(&mut ret)?;
		Ok(ret)
	}

	fn parse_fn(&mut self, f: &syn::ForeignItemFn) -> Result<(), ()> {
		let mut curfunc = SimpFunc::default();
		curfunc.access = match &f.vis {
			Visibility::Public(_) => "pub".to_string(),
			Visibility::Restricted(r) => format!("pub({})", path_to_string(&r.path)),
			Visibility::Inherited => String::new(),
		};
		curfunc.is_async = f.sig.asyncness.is_some();
		curfunc.fn_name = f.sig.ident.to_string();

		let mut ns = String::new();
		for attr in &f.attrs {
			self.parse_attr(attr, &mut ns, &mut curfunc);
		}
		if !ns.is_empty() {
			if curfunc.klsname.is_empty() {
				curfunc.fn_name = format!("{}::{}", ns, curfunc.fn_name);
			} else {
				curfunc.klsname = format!("{}::{}", ns, curfunc.klsname);
			}
		}

		curfunc.ret = self.parse_ret(&f.sig.output)?;

		for input in &f.sig.inputs {
			match input {
				FnArg::Typed(pt) => {
					let name = pat_name(&pt.pat);
					let mut arg = self.parse_arg_type(&name, &pt.ty)?;
					if let Err(_) = self.build_as_c_arg(&mut arg) {
						let x = move_obj(&mut self.err_str);
						self.err_str = format!("function {} error: {x}", curfunc.fn_name);
						return Err(());
					}
					curfunc.arg_list.push(arg);
				}
				FnArg::Receiver(_) => {
					self.err_str = format!("function {}: `self` receiver is not supported", curfunc.fn_name);
					return Err(());
				}
			}
		}

		self.funcs.push(curfunc);
		Ok(())
	}

	pub fn parse_ts(self: &mut Self, input: TokenStream) -> Result<(), &str> {
		let fm: ItemForeignMod = match syn::parse2(input) {
			Ok(x) => x,
			Err(e) => {
				self.err_str = format!("macro expect extern \"C\"/\"C++\" {{...}}: {}", e);
				return Err(&self.err_str);
			}
		};
		self.is_cpp = fm.abi.name.as_ref().map(|n| n.value() == "C++").unwrap_or(false);

		for item in &fm.items {
			match item {
				ForeignItem::Fn(f) => {
					if let Err(_) = self.parse_fn(f) {
						return Err(&self.err_str);
					}
				}
				_ => {
					self.err_str = "only function declarations are supported inside the bridge block".to_string();
					return Err(&self.err_str);
				}
			}
		}
		Ok(())
	}
}

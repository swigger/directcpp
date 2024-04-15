use std::collections::HashMap;
use std::sync::Mutex;
use crate::util::{move_obj, select_val};

#[derive(Default, Debug, Copy, Clone)]
pub enum ClassHint {
	#[default]
	NoHint,
	WeakStruct,
	WeakClass,
	StrongStruct,
	StrongClass,
}

lazy_static::lazy_static! {
	static ref CLASS_HINTS: Mutex<HashMap<String, ClassHint>> = Mutex::new(HashMap::new());
}

pub fn set_class_hint(name: &str, hint: ClassHint) -> Result<(), &'static str> {
	let mut hints = CLASS_HINTS.lock().unwrap();
	if let Some(&oldh) = hints.get(name) {
		match (oldh, hint) {
			(ClassHint::StrongClass, ClassHint::StrongClass) => return Ok(()),
			(ClassHint::StrongStruct, ClassHint::StrongStruct) => return Ok(()),
			(ClassHint::WeakClass, ClassHint::WeakClass) => return Ok(()),
			(ClassHint::WeakStruct, ClassHint::WeakStruct) => return Ok(()),
			(ClassHint::StrongClass, _) => return Err("class hint conflict"),
			(ClassHint::StrongStruct, _) => return Err("class hint conflict"),
			(ClassHint::WeakClass, ClassHint::WeakStruct) => return Err("class hint conflict"),
			(ClassHint::WeakStruct, ClassHint::WeakClass) => return Err("class hint conflict"),
			_ => hints.insert(name.to_string(), hint),
		};
	} else {
		hints.insert(name.to_string(), hint);
	}
	Ok(())
}

#[derive(Default, Clone)]
pub struct SimpArg{
	pub raw_str: String,
	pub name: String,
	pub tp: String,
	pub tp_wrap: String,
	pub tp_full: String,
	pub tp_asc: String,  // as c arg in rust expression.
	pub tp_cpp: String,  // the guessed type in cpp
	pub is_const: bool,
	pub is_primitive: bool,
}

#[derive(Default, Clone)]
pub struct SimpFunc {
	pub access: String,
	pub klsname: String,
	pub fn_name: String,
	pub template_types: Vec<String>,
	pub arg_list: Vec<SimpArg>,
	pub ret: SimpArg,
	pub is_const: bool,  // const member function
}


#[derive(Default)]
struct MSVCMangler{
	sout: String,
	is64: bool,
	cache: HashMap<String, usize>,
}

impl MSVCMangler {
	fn new() -> Self {
		Self{sout: String::new(), is64:cfg!(target_pointer_width = "64"), cache: HashMap::new()}
	}
	fn class_flag(tp: &str) -> char {
		if let Some(&x) = CLASS_HINTS.lock().unwrap().get(tp) {
			match x {
				ClassHint::WeakStruct|ClassHint::StrongStruct => return 'U',
				ClassHint::WeakClass|ClassHint::StrongClass => return 'V',
				_ => {}
			}
		}
		// panic!("class hint not set: {}", tp);
		if tp.starts_with("C") || tp.ends_with("Class") {
			'V'
		} else {
			'U'
		}
	}
	fn map_tp(intp: &str) -> &'static str{
		match intp {
			"i32"|"int" => "H",
			"u32"|"uint32_t" => "I",
			"i64"|"int64_t" => "_J",
			"u64"|"uint64_t" => "_K",
			"bool" => "_N",
			"i8"|"int8_t" => "C",
			"u8"|"uint8_t" => "E",
			"i16"|"int16_t" => "F",
			"u16"|"uint16_t" => "G",
			"f32"|"float" => "M",
			"f64"|"double" => "N",
			""|"()"|"void" => "X",
			_ => "",
		}
	}
	fn add_type(self: &mut Self, tp: &str, _is_const: bool) -> Result<(), &'static str> {
		_ = &self.cache;
		let reg1 = r"\s*(const\s+)?(.*?)([&*])\s*$";
		let reg2 = r"\s*(?:std::\s*)?(\w+)<(\w+)>\s*$";

		if let Some(caps) = regex::Regex::new(reg1).unwrap().captures(tp) {
			self.sout.push_str(match &caps[3] {
				"&" => "A",
				"*" => "P",
				_ => "",
			});
			if self.is64 {
				self.sout.push('E');
			}
			if caps.get(1).is_none() || caps[1].is_empty() {
				self.sout.push('A');
			} else {
				self.sout.push('B');
			}
			return self.add_type(&caps[2], false);
		}

		if let Some(caps) = regex::Regex::new(reg2).unwrap().captures(tp) {
			self.sout.push('V');  // class, U for struct
			self.sout.push_str("?$");
			self.sout.push_str(&caps[1]);
			self.sout.push_str("@");
			let extra_ns= match &caps[1] {
				"shared_ptr"|"unique_ptr" => "std",
				_ => ""
			};
			self.sout.push(Self::class_flag(&caps[2]));
			self.sout.push_str(&caps[2]);
			self.sout.push_str("@@@");
			if extra_ns.len() > 0 {
				self.sout.push_str(extra_ns);
				self.sout.push('@');
			}
			self.sout.push('@');
			return Ok(());
		}

		let tpstr = Self::map_tp(tp);
		if ! tpstr.is_empty() {
			self.sout.push_str(tpstr);
			return Ok(());
		}
		// treat as UDT
		self.sout.push(Self::class_flag(tp));
		self.sout.push_str(tp);
		self.sout.push_str("@@");
		return Ok(());
	}
	pub fn add_name(self: &mut Self, name:&str, template_types: Option<&Vec<String>>)
		-> Result<(), &'static str>
	{
		if let Some(template_types) = template_types {
			if template_types.len() > 0 {
				self.sout.push_str("?$");
				self.sout.push_str(name);
				self.sout.push('@');
				for tp in template_types {
					self.add_type(tp, false)?;
				}
				self.sout.push('@');
				return Ok(());
			}
		}
		self.sout.push_str(name);
		self.sout.push('@');
		Ok(())
	}
	pub fn mangle(self: &mut Self, func: &SimpFunc) -> Result<String, &'static str> {
		self.sout.push('?');
		let mut parts= func.fn_name.split("::").collect::<Vec<_>>();
		parts.reverse();
		self.add_name(parts[0], Some(&func.template_types))?;
		for idx in 1..parts.len() {
			self.add_name(parts[idx], None)?;
		}

		if ! func.klsname.is_empty() {
			self.sout.push_str(&func.klsname);
			self.sout.push('@');
			self.sout.push('@');
			self.sout.push('Q'); // public
			if self.is64 { // ptr64
				self.sout.push('E');
			}
			if func.is_const {
				self.sout.push('B');
			} else {
				self.sout.push('A');
			}
		} else {
			self.sout.push('@'); // end of name
			self.sout.push('Y'); // global
		}

		self.sout.push('A');  // cdecl
		if ! func.ret.is_primitive {
			self.sout.push_str("?A");  // return storage class: empty. non-cv
		}
		self.add_type(&func.ret.tp_cpp, false)?;

		if func.arg_list.is_empty() {
			self.sout.push('X');
		} else {
			for arg in &func.arg_list {
				self.add_type(&arg.tp_cpp, arg.is_const)?;
			}
			self.sout.push('@');
		}
		self.sout.push('Z');
		Ok(move_obj(&mut self.sout))
	}
}

#[derive(Default)]
struct GccMangler {
	sout: String,
	is64: bool,
}

impl GccMangler{
	fn new()->Self{
		Self{sout:String::new(), is64: cfg!(target_pointer_width = "64")}
	}
	fn add_source_name(self: &mut Self, name: &str) {
		self.sout.push_str(&format!("{}{}", name.len(), name));
	}
	fn add_type(self: &mut Self, tp: &str) {
		let reg = r"(const\s+)?(\w+)\s*([&*])";
		if let Some(caps) = regex::Regex::new(reg).unwrap().captures(tp) {
			if &caps[3] == "&" {
				self.sout.push('R');
			} else if &caps[3] == "*" {
				self.sout.push('P');
			}
			if caps.get(1).is_none() || caps[1].is_empty() {
				// self.sout.push('C');
			} else {
				self.sout.push('K');
			}
			self.add_type(&caps[2]);
			return;
		} else {
			match tp {
				"i32"|"int" => self.sout.push('i'),
				"u32"|"uint32_t" => self.sout.push('j'),
				"i64"|"int64_t" => self.sout.push(select_val(self.is64,'l', 'x')),
				"u64"|"uint64_t" => self.sout.push(select_val(self.is64,'m', 'y')),
				"bool" => self.sout.push('b'),
				"i8"|"int8_t" => self.sout.push('a'),
				"u8"|"uint8_t" => self.sout.push('h'),
				"i16"|"int16_t" => self.sout.push('s'),
				"u16"|"uint16_t" => self.sout.push('t'),
				"f32"|"float" => self.sout.push('f'),
				"f64"|"double" => self.sout.push('d'),
				""|"()"|"void" => self.sout.push('v'),
				_ => self.add_source_name(tp),
			}
		}
	}

	fn add_source_name_n(self: &mut Self, name1: &str, name2: &str, tt: Option<&Vec<String>>) -> Result<bool,&'static str> {
		let mut v = name1.split("::")
			.filter(|x| !x.is_empty())
			.collect::<Vec<_>>();
		v.extend(name2.split("::").filter(|x| !x.is_empty()));
		let need_e = match v.len() {
			0 => { return Err("empty name")},
			1 => { self.add_source_name(v[0]); false },
			2 if v[0] == "std" => {
				self.sout.push_str("St");
				self.add_source_name(v[1]);
				false
			},
			_ => {
				self.sout.push('N');
				let mut idx = 0;
				for x in v {
					if x == "std" && idx == 0 {
						self.sout.push_str("St");
					} else {
						self.add_source_name(x);
					}
					idx += 1;
				}
				true
			}
		};
		let mut has_temp = false;
		if let Some(tt) = tt {
			if tt.len() > 0 {
				self.sout.push('I');
				for x in tt {
					self.add_source_name(x);
				}
				self.sout.push('E');
				has_temp = true;
			}
		}
		if need_e {
			self.sout.push('E');
		}
		Ok(has_temp)
	}

	pub fn mangle(self: &mut Self, func: &SimpFunc) -> Result<String, &'static str> {
		self.sout.push_str("_Z");
		let show_ret = self.add_source_name_n(&func.klsname, &func.fn_name, Some(&func.template_types))?;
		if show_ret {
			self.add_type(&func.ret.tp_cpp);
		}
		if func.arg_list.is_empty() {
			self.sout.push('v');
		} else {
			for arg in &func.arg_list {
				self.add_type(&arg.tp_cpp);
			}
		}
		Ok(move_obj(&mut self.sout))
	}
}

pub fn mangle_msvc(func: &SimpFunc) -> Result<String, &'static str> {
	MSVCMangler::new().mangle(func)
}

pub fn mangle_gcc(func: &SimpFunc) -> Result<String, &'static str> {
	GccMangler::new().mangle(func)
}

pub fn mangle(func:&SimpFunc) -> Result<String, &'static str> {
	#[cfg(not(target_os = "windows"))]
	{ return GccMangler::new().mangle(func); }
	#[cfg(target_os = "windows")]
	{ return MSVCMangler::new().mangle(func); }
}

pub fn dtor_name(tp: &str) -> String {
	let mut func = SimpFunc::default();
	func.fn_name = "ffi::man_dtor".to_string();
	func.template_types.push(tp.to_string());
	func.ret.is_primitive = true;
	let mut arg = SimpArg::default();
	arg.tp_cpp = "void*".to_string();
	func.arg_list.push(arg);
	mangle(&func).unwrap()
}

pub fn sp_dtor_name(tp: &str) -> String {
	let mut func = SimpFunc::default();
	func.fn_name = "ffi::man_dtor_sp".to_string();
	func.template_types.push(tp.to_string());
	func.ret.is_primitive = true;
	let mut arg = SimpArg::default();
	arg.tp_cpp = "void*".to_string();
	func.arg_list.push(arg);
	mangle(&func).unwrap()
}

// I don't know why these are warning as unused. they're used in other files.
#[allow(dead_code)]
fn kill_warnings() {
	let _ = mangle_gcc;
	let _ = mangle_msvc;
	let _ = (ClassHint::StrongClass, ClassHint::StrongStruct,
	         ClassHint::WeakClass, ClassHint::WeakStruct, ClassHint::NoHint);
	let _ = set_class_hint;
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_mangle() {
		let mut func = SimpFunc::default();
		func.fn_name = "foo".to_string();
		let res = mangle_gcc(&func).unwrap();
		assert_eq!(res, "_Z3foov");

		{
			let tp = "Foo";
			let mut func = SimpFunc::default();
			func.fn_name = "ffi::man_dtor".to_string();
			func.template_types.push(tp.to_string());
			func.ret.is_primitive = true;
			let mut arg = SimpArg::default();
			arg.tp_cpp = "void*".to_string();
			func.arg_list.push(arg);
			assert_eq!(mangle_msvc(&func), Ok("??$man_dtor@UFoo@@@ffi@@YAXPEAX@Z".to_string()));
			assert_eq!(mangle_gcc(&func), Ok("_ZN3ffi8man_dtorI3FooEEvPv".to_string()));
		}
	}
}

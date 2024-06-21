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
	pub is_async: bool,
}


#[derive(Default)]
pub struct MSVCMangler{
	sout: String,
	is64: bool,
}

impl MSVCMangler {
	fn new() -> Self {
		Self{sout: String::new(), is64:cfg!(target_pointer_width = "64")}
	}
	fn class_flag(tp: &str) -> char {
		if let Some(&x) = CLASS_HINTS.lock().unwrap().get(tp) {
			match x {
				ClassHint::WeakStruct|ClassHint::StrongStruct => return 'U',
				ClassHint::WeakClass|ClassHint::StrongClass => return 'V',
				_ => {}
			}
		}
		// check well-known types
		match tp {
			"shared_ptr"|"unique_ptr" => 'V',
			"RustVec"|"RustString" => 'U',
			_ => {
				// panic!("class hint not set: {}", tp);
				if tp.starts_with("C") || tp.ends_with("Class") {
					'V'
				} else {
					'U'
				}
			}
		}
	}
	fn map_tp(intp: &str, is64: bool) -> &'static str{
		match intp {
			"i32"|"int" => "H",
			"u32"|"uint32_t" => "I",
			"i64"|"int64_t" => "_J",
			"u64"|"uint64_t" => "_K",
			"bool" => "_N",
			"char" => "D",
			"size_t" => select_val(is64, "_K", "K"),
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
			self.sout.push(Self::class_flag(&caps[1]));  // class, U for struct
			self.sout.push_str("?$");
			self.sout.push_str(&caps[1]);
			self.sout.push_str("@");
			let extra_ns= match &caps[1] {
				"shared_ptr"|"unique_ptr" => "std",
				_ => ""
			};
			self.add_type(&caps[2], false)?;
			self.sout.push_str("@");
			if extra_ns.len() > 0 {
				self.sout.push_str(extra_ns);
				self.sout.push('@');
			}
			self.sout.push('@');
			return Ok(());
		}

		let tpstr = Self::map_tp(tp, self.is64);
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
			let mut cache = HashMap::new();
			let mut cache_idx = 0;
			for arg in &func.arg_list {
				let old_sz = self.sout.len();
				self.add_type(&arg.tp_cpp, arg.is_const)?;
				let added = (&self.sout[old_sz..]).to_string();
				if let Some(&idx) = cache.get(&added) {
					self.sout.truncate(old_sz);
					let idx_str = format!("{}", idx);
					self.sout.push_str(&idx_str);
				} else if added.len() > 1 {
					cache.insert(added, cache_idx);
					cache_idx += 1;
				}
			}
			self.sout.push('@');
		}
		self.sout.push('Z');
		Ok(move_obj(&mut self.sout))
	}
}

#[derive(Default)]
pub struct GccMangler {
	sout: String,
	is64: bool,
	subs: HashMap<String, usize>,
	subs_cnt: usize,
}

impl GccMangler{
	pub fn new()->Self{
		Self {
			sout:String::new(),
			is64: cfg!(target_pointer_width = "64"),
			subs: HashMap::new(),
			subs_cnt: 0,
		}
	}
	fn format_radix(mut x: u128, radix: u32) -> String {
		let mut result = vec![];

		loop {
			let m = x % radix as u128;
			x = x / radix as u128;

			// will panic if you use a bad radix (< 2 or > 36).
			result.push(std::char::from_digit(m as u32, radix).unwrap());
			if x == 0 {
				break;
			}
		}
		result.into_iter().rev().collect()
	}

	fn add_source_name(outs:&mut String, name: &str) {
		outs.push_str(&format!("{}{}", name.len(), name));
	}
	fn add_type0(&self, vouts: &mut Vec<String>, tp: &str, cvflag: i32) {
		let reg1 = r"\s*(const\s+)?(.*?)([&*])\s*$";
		let reg2 = r"\s*(std::\s*)?(\w+)<(\w+)>\s*$";
		let mut outs = String::new();
		if (cvflag & 1) != 0 {
			outs.push('K');
		}
		if (cvflag & 2) != 0 {
			outs.push('V');
		}
		if let Some(caps) = regex::Regex::new(reg1).unwrap().captures(tp) {
			if &caps[3] == "&" {
				outs.push('R');
			} else if &caps[3] == "*" {
				outs.push('P');
			}
			vouts.push(outs);
			if caps.get(1).is_none() || caps[1].is_empty() {
				self.add_type0(vouts, &caps[2], 0);
			} else {
				self.add_type0(vouts, &caps[2], 1);
			}
			return;
		}
		if let Some(caps) = regex::Regex::new(reg2).unwrap().captures(tp) {
			let mut vouts1 = Vec::new();
			self.add_type0(&mut vouts1, &caps[2], 0);
			vouts1.push("I".to_string());
			self.add_type0(&mut vouts1, &caps[3], 0);
			vouts1.push("E".to_string());
			vouts.push(vouts1.join(""));
			return;
		}

		{
			match tp {
				"i32"|"int" => outs.push('i'),
				"u32"|"uint32_t" => outs.push('j'),
				"i64"|"int64_t" => outs.push(select_val(self.is64,'l', 'x')),
				"u64"|"uint64_t" => outs.push(select_val(self.is64,'m', 'y')),
				"bool" => outs.push('b'),
				"char" => outs.push('c'),
				"size_t" => outs.push(select_val(self.is64,'m', 'j')),
				"i8"|"int8_t" => outs.push('a'),
				"u8"|"uint8_t" => outs.push('h'),
				"i16"|"int16_t" => outs.push('s'),
				"u16"|"uint16_t" => outs.push('t'),
				"f32"|"float" => outs.push('f'),
				"f64"|"double" => outs.push('d'),
				""|"()"|"void" => outs.push('v'),
				_ => {
					Self::add_source_name(&mut outs, tp);
				},
			}
			vouts.push(outs);
		}
	}
	fn add_type(self: &mut Self, tp: &str) {
		let mut vouts = Vec::new();
		self.add_type0(&mut vouts, tp, 0);
		let mut orig_type = String::new();
		let mut repl_type = String::new();
		for i in 0..vouts.len() {
			let from = vouts.len()-1-i;
			let itm = vouts[from].as_str();
			orig_type = format!("{}{}", itm, orig_type.as_str());
			repl_type = format!("{}{}", itm, repl_type.as_str());
			if i == 0 && itm.len() == 1 {
				// primitive type
				continue;
			}
			if let Some(&idx) = self.subs.get(orig_type.as_str()) {
				if idx == 0 {
					repl_type = "S_".to_string();
				} else {
					repl_type = format!("S{}_", Self::format_radix((idx-1) as u128, 36));
				}
			} else {
				self.subs.insert(orig_type.clone(), self.subs_cnt);
				self.subs_cnt += 1;
			}
		}
		self.sout.push_str(repl_type.as_str());
	}

	fn add_source_name_n(self: &mut Self, name1: &str, name2: &str, tt: Option<&Vec<String>>) -> Result<bool,&'static str> {
		let mut v = name1.split("::")
			.filter(|x| !x.is_empty())
			.collect::<Vec<_>>();
		v.extend(name2.split("::").filter(|x| !x.is_empty()));
		let need_e = match v.len() {
			0 => { return Err("empty name")},
			1 => { Self::add_source_name(&mut self.sout, v[0]); false },
			2 if v[0] == "std" => {
				self.sout.push_str("St");
				Self::add_source_name(&mut self.sout, v[1]);
				false
			},
			_ => {
				self.sout.push('N');
				let mut idx = 0;
				for x in v {
					if x == "std" && idx == 0 {
						self.sout.push_str("St");
					} else {
						Self::add_source_name(&mut self.sout, x);
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
					let re = regex::Regex::new(r"(.*?)<(.*)>").unwrap();
					if let Some(caps) = re.captures(x) {
						let tps = caps[2].split(',').map(|x| x.to_string()).collect::<Vec<_>>();
						self.add_source_name_n("",&caps[1], Some(&tps))?;
					} else {
						self.add_type(x);
					}
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
		self.subs.clear();
		self.subs_cnt = 0;
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
	// arguments may not 1:1
	let mut func2 = func.clone();
	func2.arg_list.clear();
	for args in  &func.arg_list {
		if let Some(_) = args.tp_cpp.find(',') {
			for seg in args.tp_cpp.split(',') {
				let mut arg = args.clone();
				arg.tp_cpp = seg.to_string();
				func2.arg_list.push(arg);
			}
		} else {
			func2.arg_list.push(args.clone());
		}
	}
	#[cfg(not(target_os = "windows"))]
	{ return GccMangler::new().mangle(&func2); }
	#[cfg(target_os = "windows")]
	{ return MSVCMangler::new().mangle(&func2); }
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

	fn add_arg(f: &mut SimpFunc, tp: &str, name: &str) {
		let mut arg = SimpArg::default();
		arg.tp_cpp = tp.to_string();
		arg.name = name.to_string();
		arg.is_primitive = match tp {
			"int8_t"|"uint8_t" => true,
			"int16_t"|"uint16_t" => true,
			"int"|"int32_t"|"uint32_t" => true,
			"int64_t"|"uint64_t" => true,
			"bool" => true,
			"char" => true,
			"size_t" => true,
			"float" => true,
			"double" => true,
			"void" => true,
			_ => false,
		};
		f.arg_list.push(arg);
	}
	fn set_ret(f: &mut SimpFunc, tp:&str) {
		let mut f2 = SimpFunc::default();
		add_arg(&mut f2, tp, "ret");
		f.ret = f2.arg_list[0].clone();
	}

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
		{
			let mut func = SimpFunc::default();
			func.fn_name = "cpp_ptr".to_string();
			set_ret(&mut func, "void");
			add_arg(&mut func, "int", "a");
			add_arg(&mut func, "const char*", "b");
			add_arg(&mut func, "size_t", "c");
			add_arg(&mut func, "const char*", "d");
			add_arg(&mut func, "const uint8_t*", "e");
			add_arg(&mut func, "size_t", "f");
			assert_eq!(mangle_gcc(&func), Ok("_Z7cpp_ptriPKcmS0_PKhm".to_string()));
		}
		{
			let mut func = SimpFunc::default();
			func.fn_name = "slow_tostr".to_string();
			set_ret(&mut func, "void");
			add_arg(&mut func, "ValuePromise<RustString>*", "val");
			add_arg(&mut func, "int", "val");
			assert_eq!(mangle_gcc(&func), Ok("_Z10slow_tostrP12ValuePromiseI10RustStringEi".to_string()));
		}
	}
}

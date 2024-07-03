#![cfg(test)]
use quote::quote;
use super::*;

fn to_string(ts: TokenStream) -> String {
	let ins = ts.to_string();
	let mut outs = String::with_capacity(ins.len());
	let mut last_ch = None;
	let mut need_sp = false;
	let mut in_quote = 0;
	for ch in ins.chars() {
		if (ch == '\'') && (in_quote == 0 || in_quote == 1) {
			in_quote = select_val(in_quote == 0, 1, 0);
			outs.push(ch);
			need_sp = false;
		} else if (ch == '"') && (in_quote == 0 || in_quote == 2) {
			if in_quote == 0 && need_sp {
				if let Some(ch1) = last_ch {
					outs.push(ch1);
				}
			}
			in_quote = select_val(in_quote == 0, 2, 0);
			outs.push(ch);
			need_sp = false;
		} else if ch == '[' && in_quote == 0 {
			in_quote = 3;
			outs.push(ch);
			need_sp = false;
		} else if ch == ']' && in_quote == 3 {
			in_quote = 0;
			outs.push(ch);
			need_sp = false;
		} else if in_quote == 1 || in_quote == 2 {
			outs.push(ch);
			last_ch = None;
		} else if ch.is_ascii_whitespace() {
			last_ch = Some(ch);
			continue;
		} else if ch != '_' && ch.is_ascii_punctuation() {
			outs.push(ch);
			if ch == '{' || ch == '}' || (ch == ';' && in_quote==0) {
				outs.push('\n');
			}
			last_ch = None;
			need_sp = false;
		} else {
			if let Some(ch1) = last_ch {
				if need_sp {
					outs.push(ch1);
				}
			}
			outs.push(ch);
			last_ch = None;
			need_sp = true;
		}
	}
	outs
}

#[test]
fn test_retvec() {
	let input = quote::quote! {
		extern "C++" {
			pub fn init_asset_config(url: &CStr, olddata: &[u8]) -> Vec<u8>;
		}
	};
	let mut bb = FFIBuilder::new();
	let os = to_string(bb.build_bridge_code(input).unwrap());
	println!("{}", os);

	let mut func = SimpFunc::default();
	func.fn_name = "ffi::man_dtor".to_string();
	func.template_types.push("RustVec<uint8_t>".to_string());
	func.ret.is_primitive = true;
	let mut arg = SimpArg::default();
	arg.tp_cpp = "void*".to_string();
	func.arg_list.push(arg);
	let s = crate::mangle::GccMangler::new().mangle(&func).unwrap();
	assert_eq!(s.as_str(), "_ZN3ffi8man_dtorI7RustVecIhEEEvPv");
}

#[test]
fn test_cstr() {
	let input = quote::quote! {
		extern "C++" {
			#[namespace(ns_foo::ns_bar)]
			pub fn cpp_ptr(foo:&CStr, bar:&str, baz:&[u8]) -> i32;
		}
	};
	let mut bb = FFIBuilder::new();
	let expect = quote::quote! {
		extern "C" {
			#[link_name="?cpp_ptr@ns_bar@ns_foo@@YAHPEBD0_KPEBE1@Z"]
			fn ffi__cpp_ptr(foo:*const i8,bar:*const u8,bar_len:usize,baz:*const u8,baz_len:usize)->i32;
		}
		pub fn cpp_ptr(foo:&CStr,bar:&str,baz:&[u8])->i32 {
			unsafe { ffi__cpp_ptr(foo.as_ptr(),bar.as_ptr(),bar.len(),baz.as_ptr(),baz.len()) }
		}
	};
	assert_eq!(to_string(bb.build_bridge_code(input).unwrap()), to_string(expect));
}

#[test]
fn test_async() {
	let input_ts = quote::quote! {
		extern "C++" {
			pub async fn future_int() -> i32;
		}
	};
	let mut bb = FFIBuilder::new();
	let r1 = bb.build_bridge_code(input_ts);
	assert!(r1.is_ok());
	let r1s = to_string(r1.unwrap());
	// cpp type: void future_int(ValuePromise<int>* ret, ...);
	let expect = quote::quote! {
		extern "C"{
			#[link_name="?future_int@@YAXPEAU?$ValuePromise@H@@@Z"]
			fn ffi__future_int(addr: usize);
		}
		pub async fn future_int() -> i32 {
			let mut arr : [usize;2] = [0, 0];
			let mut fv = std::pin::pin!(FutureValue::<i32>::default());
			let dyn_fv_addr = fv.copy_vtbl(&mut arr);
			unsafe { ffi__future_int(dyn_fv_addr); }
			fv.await
		}
	};
	println!("{}", r1s);
	assert_eq!(r1s, to_string(expect));
}

#[test]
fn test_pod() {
	let input_ts = quote::quote! {
		extern "C++" {
			pub fn get_logger() -> POD<DynLogger>;
		}
	};
	let mut bb = FFIBuilder::new();
	let r1 = bb.build_bridge_code(input_ts);
	if let Err(es) = r1 {
		println!("{}", es);
	}
	assert!(r1.is_ok());
	let r1s = to_string(r1.unwrap());
	let expected = quote::quote! {
		extern "C"{
			#[link_name="?get_logger@@YA?AUDynLogger@@XZ"]
			fn ffi__get_logger(__rto:*mut usize);
		}
		pub fn get_logger()->DynLogger{
			const SZ:usize=(std::mem::size_of::<DynLogger>()+16)/8;
			let mut __rta:[usize;SZ]=[0;SZ];
			unsafe{
				ffi__get_logger(&mut __rta as*mut usize);
				let __rto=(*(&__rta as*const usize as*const DynLogger)).clone();
				__rto
			}
		}
	};
	assert_eq!(r1s, to_string(expected));
}

#[test]
fn it_works() {
	let ts = quote::quote!(
		extern "C++" {
			pub fn cpp_ptr(xx:i32) -> SharedPtr<CppStruct>;
		}
	);
	let resp = quote! {
		extern "C" {
			#[link_name = "?cpp_ptr@@YA?AV?$shared_ptr@VCppStruct@@@std@@H@Z"]
			fn ffi__cpp_ptr (__rto : * mut u8 , xx : i32) ;
			#[link_name = "??$man_dtor_sp@VCppStruct@@@ffi@@YAXPEAX@Z"]
			fn ffi__freeSP_CppStruct (__o : * mut usize) ;
		}
		impl DropSP for CppStruct {
			unsafe fn __drop_sp (ptr : * mut [u8;0]) {
				if ptr as usize != 0 {
					ffi__freeSP_CppStruct (ptr as * mut usize) ;
				}
			}
		}
		pub fn cpp_ptr (xx : i32) -> SharedPtr<CppStruct> {
			let mut __rto = SharedPtr::<CppStruct>::default();
			unsafe {
				ffi__cpp_ptr (&mut __rto as * mut SharedPtr<CppStruct> as * mut u8, xx);
			}
			__rto
		}
	};

	let mut bb = FFIBuilder::new();
	let r1 = bb.build_bridge_code(ts);
	assert!(r1.is_ok());
	let r1 = to_string(r1.unwrap());
	assert_eq!(r1, to_string(resp));

	let ts = quote::quote!(
		extern "C++" {
			pub fn on_magic(magic: &mut MagicIn, cs: CPtr<CppStruct>) -> MagicOut;
		}
	);
	let expect = quote::quote! {
		extern "C" {
			#[link_name="?on_magic@@YA?AUMagicOut@@AEAUMagicIn@@PEAVCppStruct@@@Z"]
			fn ffi__on_magic(__rto:*mut usize,magic:*mut MagicIn,cs:*const u8);
			#[link_name="??$man_dtor@UMagicOut@@@ffi@@YAXPEAX@Z"]
			fn ffi__free_MagicOut(__o:*mut usize);
		}
		pub fn on_magic(magic:&mut MagicIn,cs:CPtr<CppStruct>)->MagicOut{
			const SZ:usize=(std::mem::size_of::<MagicOut>()+16)/8;
			let mut __rta:[usize;SZ]=[0;SZ];
			unsafe {
				ffi__on_magic(&mut __rta as*mut usize,magic as*mut MagicIn,cs.addr as*const u8);
				let __rto=(*(&__rta as*const usize as*const MagicOut)).clone();
				ffi__free_MagicOut(&mut __rta as*mut usize);
				__rto
			}
		}
	};
	assert_eq!(to_string( bb.build_bridge_code(ts).unwrap()   ), to_string(expect));
	let ts = quote::quote!(
		extern "C++" {
			#[member_of(CppStruct)]
			pub fn AddString(str: &String);
			#[member_of(CppStruct)]
			pub fn get_order(oid: i32) -> UserOrder;
			pub fn fooo(xx:i32)->SharedPtr<CppStruct>;
			pub fn start_ctp(ac: &AccountInfo) -> i32;
			pub fn set_log_verbose(v: i32);
			pub fn new_user_order_c(order: &UserOrder, xxx: i32) -> UserOrderResp;
		}
	);
	println!("{}", to_string(bb.build_bridge_code(ts).unwrap()));
}

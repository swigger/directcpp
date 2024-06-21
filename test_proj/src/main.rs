use core::pin::Pin;
use directcpp::{SharedPtr, DropSP, CPtr, AsCPtr, ValuePromise, FutureValue};
use std::ffi::{CStr, CString};
use tokio::runtime::Runtime;

#[repr(C)]
struct MagicIn{
	ivalue: i32,
	fvalue: f32,
	svalue: String,
}

#[repr(C)]
#[derive(Clone, Debug)]
struct MagicOut{
	ivalue: i64,
	fvalue: f64,
	sa: String,
	sb: String,
}

struct Proof;

#[directcpp::bridge]
extern "C++" {
	// for simple functions, it's easy to go.
	pub fn on_start();

	// for returning normal objects, it must be cloneable, rust will clone a copy and drop the original
	// this is because cpp/rust may have different memory models and resource management on
	// the heap. And then normal object as arguments can only be readonly shared between cpp/rust.
	// ok, you can use &mut MagicIn here, at your own risk.
	pub fn on_magic(magic: &MagicIn) -> MagicOut;
	#[namespace(myns)]
	pub fn get_message()->String;

	// for complex object can only be handled at c++ side.
	// rust will keep a reference to the shared_ptr
	pub fn cpp_ptr(a0: i32, xx:&str, xx2:&CStr, xx3:&[u8]) -> SharedPtr<Proof>;

	pub fn get_bin() -> Vec<u8>;

	pub async fn slow_tostr(val: i32) -> String;

	// for complex objects that can only be handled at rust side,
	// we can always pass its address to cpp side via void* aka *const u8.
	// so there is nothing special to do here.

	// for member functions, we have another kind of magic.
	#[member_of(Proof)]
	pub fn foo();
	#[member_of(Proof)]
	pub fn AddString(str: &String);
	#[member_of(Proof)]
	pub fn Print();
}

// for msvc-friendly we should link the debug library in the debug mode
// rust itself does not support to link with msvcrtd.lib,
// so we'll force it to do so in a hacker's way and this is the MAGIC!
#[directcpp::enable_msvc_debug]
struct Unused;

async fn simple_async_func() {
	println!("Rust: simple_async_func start!");
	let s = slow_tostr(42).await;
	println!("Rust: simple_async_func end with s={}", s);
}

fn main()
{
	println!("Hello from rust!");
	// let's call the on_start function in CPP!
	on_start();
	println!("{}", get_message());

	/*
	let mut x: u64 = 4;
	unsafe {
		asm!(
		"mov {tmp}, {x}",
		"shl {tmp}, 1",
		"shl {x}, 2",
		"add {x}, {tmp}",
		x = inout(reg) x,
		tmp = out(reg) _,
		);
	}
	assert_eq!(x, 4 * 6);
	*/

	// lets got the struct from cpp and call some member.
	let xx = cpp_ptr(42, "Hello from Rust!", &CString::new("Hello from Rust!").unwrap(), &[1,2,3,4,5]);
	println!("\x1b[1;34mRust: got shared_ptr, will call member!\x1b[0m");
	Proof__foo(xx.as_cptr());
	Proof__AddString(xx.as_cptr(), &"Hello from Rust!".to_string());
	Proof__Print(xx.as_cptr());
	println!("\x1b[1;34mdropping the shared_ptr in rust!\x1b[0m");
	drop(xx);

	let bin = get_bin();
	let bin = bin.iter().map(|x| format!("{:02x}", x)).collect::<Vec<String>>().join("");
	println!("Rust: got bin: {}", bin);

	let mut msgin = MagicIn{
		ivalue: 42,
		fvalue: std::f32::consts::PI,
		svalue: "Bonjour!".to_string(),
	};

	println!("\x1b[1;34mLets do magic IO!\x1b[0m");
	let mgo = on_magic(&mut msgin);
	println!("Rust: got magic: {:?}", mgo);

	let runtime = Runtime::new().unwrap();
	runtime.block_on(simple_async_func());
}

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};
use std::task::{Context, Poll};
/// Generate bridge code for C++ functions.
/// # Examples
/// ```
/// use directcpp::bridge;
/// #[bridge]
/// extern "C++" {
/// 	pub fn on_start();
/// }
/// ```
/// This generates the following code:
/// ```
/// extern "C" {
///     #[link_name = "?on_start@@YAXXZ"]
///     fn ffi__on_start();
/// }
/// pub fn on_start() { unsafe { ffi__on_start() } }
/// ```
/// See the [README](https://github.com/swigger/directcpp/) for more details.
pub use directcpp_macro::bridge;

/// Allow link to msvc debug runtime.
/// # Examples
/// ```
/// use directcpp::enable_msvc_debug;
/// #[enable_msvc_debug]
/// struct UnusedStruct;
/// ```
/// This generates nothing, but it works silently background in a hacker's way.
///
pub use directcpp_macro::enable_msvc_debug;

// implies the value is POD so no dtor is needed, it's just copied.
#[doc(hidden)]
pub struct POD<T>(T);

#[doc(hidden)]
pub trait ManDtor {
	unsafe fn __dtor(ptr: *mut [u8;0]);
}

#[doc(hidden)]
pub trait DropSP {
	unsafe fn __drop_sp(sp_ptr: *mut [u8;0]);
}

#[repr(C)]
pub struct SharedPtr<T> where T: DropSP {
	val1: usize,
	val2: usize,
    _phantom: PhantomData<T>,
}
impl<T> Default for SharedPtr<T> where T: DropSP {
	fn default() -> Self {
		Self {
			val1: 0,
			val2: 0,
			_phantom: PhantomData,
		}
	}
}
impl<T> Drop for SharedPtr<T> where T: DropSP {
	fn drop(&mut self) {
		unsafe {
			T::__drop_sp(self as *mut SharedPtr<T> as *mut [u8;0]);
		}
	}
}

#[repr(C)]
pub struct UniquePtr<T> where T: ManDtor {
	val1: usize,
	_phantom: PhantomData<T>,
}
impl<T> Default for UniquePtr<T> where T: ManDtor {
	fn default() -> Self {
		Self {
			val1: 0,
			_phantom: PhantomData,
		}
	}
}
impl<T> Drop for UniquePtr<T> where T: ManDtor {
	fn drop(&mut self) {
		unsafe {
			if self.val1 != 0 {
				T::__dtor(self.val1 as *mut [u8; 0]);
			}
		}
	}
}

#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct CPtr<T> {
	pub addr: usize,
	_phantom: PhantomData<T>,
}

pub trait AsCPtr<T> {
	fn as_cptr(&self) -> CPtr<T>;
}

impl<T> AsCPtr<T>  for SharedPtr<T>
	where T:DropSP
{
	fn as_cptr(&self) -> CPtr<T> {
		CPtr::<T> {
			addr: self.val1,
			_phantom: PhantomData
		}
	}
}

impl<T> AsCPtr<T> for UniquePtr<T>
	where T: ManDtor
{
	fn as_cptr(&self) -> CPtr<T> {
		CPtr::<T> {
			addr: self.val1,
			_phantom: PhantomData
		}
	}
}

#[repr(C)]
struct FutureValueInner<T> {
	f_set_value: fn(usize, &T),
	value: Mutex<(Option<T>, Option<std::task::Waker>)>
}
pub struct FutureValue<T> {
	value: Arc<FutureValueInner<T>>
}

impl<T> Future for FutureValue<T> {
	type Output = T;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut lock = self.value.value.lock().unwrap();
		match lock.0.take() {
			Some(ss) => Poll::Ready(ss),
			None => {
				lock.1 = Some(cx.waker().clone());
				Poll::Pending
			}
		}
	}
}

impl<T> Default for FutureValue<T> where T:Clone {
	fn default() -> Self {
		Self {
			value: Arc::new(FutureValueInner {
				f_set_value: Self::set_value,
				value: Mutex::new((None, None))
			})
		}
	}
}

impl<T> FutureValue<T> where T: Clone {
	pub unsafe fn to_ptr(&mut self) -> usize {
		let p1 = self as *mut Self as *mut *mut AtomicUsize;
		(**p1).fetch_add(1, Ordering::Relaxed);
		(*p1) as usize
	}
	pub fn set_value(addr: usize, value: &T) {
		let con_addr = addr;
		let p1 = unsafe { &*(&con_addr as *const usize as *mut Self) };
		let mut lock = p1.value.value.lock().unwrap();
		lock.0 = Some(value.clone());
		let value_clone = p1.value.clone();
		unsafe {
			let au = addr as *mut AtomicUsize;
			(*au).fetch_sub(1, Ordering::Relaxed);
		}
		if let Some(w) = lock.1.as_ref() {
			w.wake_by_ref();
		}
		drop(value_clone);
	}
}

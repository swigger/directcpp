use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
pub use directcpp_macro::bridge;
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

pub trait ValuePromise {
	type Output;
	fn set_value(&self, value: &Self::Output);
}


struct FutureValueInner<T> {
	value: Option<T>,
	waker: Option<std::task::Waker>,
}
pub struct FutureValue<T> {
	value: Mutex<FutureValueInner<T>>
}

impl<T> Future for FutureValue<T> {
	type Output = T;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let mut lock = self.value.lock().unwrap();
		match lock.value.take() {
			Some(ss) => Poll::Ready(ss),
			None => {
				lock.waker = Some(cx.waker().clone());
				Poll::Pending
			}
		}
	}
}
impl<T> Default for FutureValue<T> {
	fn default() -> Self {
		Self {
			value: Mutex::new(FutureValueInner {
				value: None,
				waker: None,
			})
		}
	}
}

impl<T> ValuePromise for FutureValue<T> where T: Clone {
	type Output = T;

	fn set_value(&self, value: &T) {
		let mut lock = self.value.lock().unwrap();
		lock.value = Some(value.clone());
		if let Some(w) = lock.waker.as_ref() {
			w.wake_by_ref();
		}
	}
}

impl<T> FutureValue<T> where T: Clone {
	pub fn copy_vtbl(self: &mut Pin<&mut Self>, arr: &mut [usize;2]) -> usize {
		static_assertions::assert_eq_size!([usize;2], &dyn ValuePromise<Output=String>);
		let fv = self.as_ref().get_ref() as &dyn ValuePromise<Output=T>;
		let ptr = &fv as *const &dyn ValuePromise<Output=T> as *const [usize;2];
		unsafe {
			arr[0] = (*ptr)[0];
			arr[1] = (*ptr)[1];
			arr as *mut [usize;2] as usize
		}
	}
}

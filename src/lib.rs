use std::marker::PhantomData;
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

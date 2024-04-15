#pragma once
#include <stdint.h>
#include <cstring>
#include <cassert>
#include <string>
#include <memory>

// NOTE: this should be checked for versions and hosts, in real product this macro can be generated by
// build script. at least for current version and hosts, this macro is correct.
// with correct macro we can save cross-lang interops than what crate cxx does while keeping us safe.
#ifndef RUST_VEC_CONTENT
#define RUST_VEC_CONTENT(T) size_t cap = 0; T* data = 0; size_t len = 0
#endif

// we reimplement the RustVec and RustString in c++ side keeping the same memory layout.
// so that c++ can readonly process rust structure and vise-versa.
// note write operations are not allowed because they may using different memory management.
template <typename T>
struct RustVec {
	RUST_VEC_CONTENT(T);

	const T* begin() const { return data; }
	const T* end() const { return data + len; }
	T* begin() { return data; }
	T* end() { return data + len; }
	RustVec() { cap = 0; len = 0; data = 0; }
	~RustVec() { clear(true); }
	RustVec(const RustVec& ano) {
		data = nullptr;	len = cap = 0;
		__set_from(ano.data, ano.len);
	}
	RustVec(RustVec&& ano) noexcept {
		len = ano.len;
		cap = ano.cap;
		data = ano.data;
		ano.data = 0;
		ano.len = 0;
		ano.cap = 0;
	}
	RustVec& operator = (const RustVec& ano) {
		clear();
		__set_from(ano.data, ano.len);
		return *this;
	}
	RustVec& operator = (RustVec&& ano) noexcept {
		clear(true);
		std::swap(data, ano.data);
		std::swap(len, ano.len);
		std::swap(cap, ano.cap);
		return *this;
	}
	void clear(bool free_ = false) {
		if constexpr (!std::is_trivially_destructible<T>::value) {
			for (uintptr_t i = 0; i < len; ++i) {
				data[i].~T();
			}
		}
		len = 0;
		if (free_) {
			if (data) free(data);
			data = 0;
			cap = 0;
		}
	}
	void push_back(const T& t) {
		if (len == cap) {
			// we always assume T is trivially movable. that means moving it from one place to another
			// never need to call the move constructor, just memcpy is enough.
			// this is not always true for some types like std::string of gcc libstdc++.
			cap = std::max<uintptr_t>(cap * 2, 32u);
			data = (T*)realloc((void*)data, sizeof(T) * cap);
		}
		new (data + len) T(t);
		++len;
	}
protected:
	void __set_from(const T* data0, size_t len0)
	{
		if (cap < len0) {
			if (data) free(data);
			data = (T*)malloc(sizeof(T) * len0);
			cap = len0;
		}
		len = len0;
		if constexpr (std::is_trivially_copyable<T>::value) {
			if (data) memcpy(data, data0, sizeof(T) * len0);
		} else {
			for (uintptr_t i = 0; i < len0; ++i) {
				new (data + i) T(data0[i]);
			}
		}
	}
};

struct RustString : RustVec<char>
{
	~RustString() {	if (data) free(data); data=0;cap=len=0; }
	RustString() = default;
	RustString(const RustString& ano) : RustVec<char>(ano) {}
	RustString(RustString&& ano) noexcept : RustVec<char>(std::move(ano)) {}
	RustString(const char* str) {
		cap = len = 0; data = 0;
		__set_from(str, strlen(str));
	}
	RustString(const char* str, size_t len0) {
		cap = len = 0; data = 0;
		__set_from(str, len0);
	}
	const char* c_str() const {
		// note: if we are readonly using rust string, it may not be null-terminated.
		assert(data == 0 || data[len] == 0);
		return data;
	}
	std::string str() const {
		return std::string(data, len);
	}
	std::string_view view() const {
		return std::string_view(data, len);
	}

	RustString& operator=(RustString&& ano) noexcept {
		if (data) free(data);
		cap = len = 0; data = 0;
		RustVec<char>::operator=(std::move(ano));
		return *this;
	}
	RustString& operator=(const RustString& ano) {
		__set_from(ano.data, ano.len);
		return *this;
	}
	RustString& operator=(const std::string& ano) {
		__set_from(ano.c_str(), ano.length());
		return *this;
	}
	RustString& operator=(const std::string_view& ano) {
		__set_from(ano.data(), ano.length());
		return *this;
	}
	RustString& operator=(const char* cs) {
		__set_from(cs, strlen(cs));
		return *this;
	}
	bool operator == (const std::string_view& rhs) const { return view() == rhs; }
	bool operator == (const std::string& rhs) const { return view() == rhs; }
	bool operator == (const char* rhs) const { return view() == rhs; }
	bool operator != (const std::string_view& rhs) const { return view() != rhs; }
	bool operator != (const std::string& rhs) const { return view() != rhs; }
	bool operator != (const char* rhs) const { return view() != rhs; }
	// space ship operator
#if _MSVC_LANG+0 >= 202002L || __cplusplus >= 202002L
	auto operator<=> (const RustString& rhs) const {
		size_t min_len = std::min(len, rhs.len);
		int cmp = memcmp(data, rhs.data, min_len);
		return cmp == 0 ? len<=>rhs.len : (cmp<=>0);
	}
#endif
protected:
	void __set_from(const char* data0, size_t len0)
	{
		if (cap < len0) {
			if (data) free(data);
			data = (char*) malloc(len0+1);
			cap = len0;
		}
		len = len0;
		if (data) {
			memcpy(data, data0, len);
			data[len] = 0;
		}
	}
};

namespace ffi
{
	template <class T>
	void force_ref(const T& obj) {
		// A call to an external function is required to forbid gcc from optimizing `obj` away.
		// Any function uses `&obj` can do. If you don't like this, you can also call
		// free((void*)(intptr_t)("\0"[1 & (uintptr_t)&obj])) here to always do an empty free(NULL) call
		// as most modern C implementations treat free(NULL) as a no-op.
		// On windows you can also use LockResource(&obj) which is just a dummy function.
		// You can also call your own external empty function if there is such a function.
		char buf[32];
		snprintf(buf, 32, "%p", &obj);
	}
	// this is the function rust calls to destruct objects. it's templated
	// so that we should manually instantiate it for each type so that rust can find it.
	// using void* here to save length of mangled name. we are safe because name of T is still in mangled function name for template.
	template <class T>
	void man_dtor(void* obj) {
		((T*)obj)->~T();
	}
	template <class T>
	void man_dtor_sp(void* obj) {
		typedef std::shared_ptr<T> TPtr;
		((TPtr*)obj)->~TPtr();
	}
	template <class T>
	void enable_class() {
		force_ref(&man_dtor<T>);
	}

	template <class T>
	void enable_class_sp() {
		force_ref<void(*)(void*)>(&man_dtor_sp<T>);
	}
}

#pragma once
#include "rust_spt.h"

template <class T, size_t bitpos, uint8_t flipped, size_t objpos>
class RustOptionBase
{
public:
	void reset() {
		if (has_val()) {
			obj_ptr0()->~T();
			set_has_val0(false);
		}
	}
	bool is_none() const {
		return !has_val();
	}
	bool is_some() const {
		return has_val();
	}
	RustOptionBase& operator=(const RustOptionBase& other) {
		if (this != &other) {
			if (has_val() && other.has_val()) {
				*obj_ptr0() = *other.obj_ptr0();
			} else {
				reset();
				__copy(other);
			}
		}
		return *this;
	}
	RustOptionBase& operator=(RustOptionBase&& other) noexcept {
		if (this != &other) {
			reset();
			__move(std::move(other));
		}
		return *this;
	}
	bool has_val() const {
		constexpr uint8_t MASK = (1 << (bitpos % 8));
		constexpr uint8_t HAS = flipped ? 0 : MASK;
		uint8_t* p = (uint8_t*)this + bitpos/8;
		return (*p & MASK) == HAS;
	}
	void set(T&& val) noexcept {
		if (has_val()) {
			*obj_ptr0() = std::move(val);
		} else {
			new (obj_ptr0()) T(std::move(val));
			set_has_val0(true);
		}
	}
	void set(const T& val) {
		if (has_val()) {
			*obj_ptr0() = val;
		} else {
			new (obj_ptr0()) T(val);
			set_has_val0(true);
		}
	}
	const T& value_or(const T& def) const {
		if (has_val()) {
			return *obj_ptr0();
		} else {
			return def;
		}
	}
	const T* pointer() const noexcept {
		if (has_val()) {
			return obj_ptr0();
		} else {
			return nullptr;
		}
	}
	T* pointer() noexcept {
		if (has_val())  {
			return obj_ptr0();
		} else {
			return nullptr;
		}
	}
protected:
	void set_has_val0(bool v) noexcept {
		constexpr uint8_t MASK = (1 << (bitpos % 8));
		uint8_t* p = (uint8_t*)this + bitpos/8;
		if (v ^ flipped) {
			*p |= MASK;
		} else {
			*p &= ~MASK;
		}
	}
	T* obj_ptr0() const noexcept { // may not be valid if has_val() == false
		return (T*)((uint8_t*)this + objpos);
	}
	void __copy(const RustOptionBase& other) {
		if (other.has_val()) {
			new (obj_ptr0()) T(*other.obj_ptr0());
			set_has_val0(true);
		}
	}
	void __move(RustOptionBase&& other) noexcept {
		if (other.has_val()) {
			new (obj_ptr0()) T(std::move(*other.obj_ptr0()));
			set_has_val0(true);
			other.reset();
		}
	}
};

template <class T>
class RustOption : public RustOptionBase<T,0,0,alignof(T)> {
	uint8_t has_val = 0;
	typename std::aligned_storage<sizeof(T), alignof(T)>::type storage{};
public:
	RustOption() = default;
	RustOption(const RustOption& other) { this->__copy(other);}
	RustOption(RustOption&& other) noexcept { this->__move(std::move(other)); }
	~RustOption() { this->reset(); }
};
template <>
class RustOption<RustString> : public RustOptionBase<RustString, 8*sizeof(void*)-1, 1, alignof(RustString)> {
	typename std::aligned_storage<sizeof(RustString), alignof(RustString)>::type storage{};
public:
	RustOption() noexcept { set_has_val0(false); }
	RustOption(const RustOption& other) { set_has_val0(false); __copy(other); }
	RustOption(RustOption&& other) noexcept { set_has_val0(false); __move(std::move(other)); }
	~RustOption() { reset(); }
};
static_assert(sizeof(RustOption<uint32_t>) == 8, "sz req");
static_assert(sizeof(RustOption<RustString>) == 0x18, "sz req");

extern "C" {
	extern void* __rust_alloc(size_t nbytes, size_t align);
	extern void __rust_dealloc(void* ptr, size_t size, size_t align);
	extern void* __rust_realloc(void * ptr, size_t old_size, size_t align, size_t new_size);
}

#ifndef let
#define let const auto
#define letref const auto&
#define letmut auto
#define letmutref auto&
#endif

struct tmp_rust_string : RustString {
	tmp_rust_string(const string & ss) {
		data = (char*)ss.c_str();
		len = cap = ss.size();
	}
	tmp_rust_string(const std::string_view& sv) {
		data = (char*)sv.data();
		len = cap = sv.size();
	}
	~tmp_rust_string() {
		data = 0;
		len = cap = 0;
	}
	// not copyable, not movable.
	tmp_rust_string(const tmp_rust_string&) = delete;
	tmp_rust_string(tmp_rust_string&&) = delete;
	tmp_rust_string& operator= (const tmp_rust_string&) = delete;
	tmp_rust_string& operator= (tmp_rust_string&&) = delete;
};

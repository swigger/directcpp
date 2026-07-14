# directcpp — Call C++ directly from Rust

`directcpp` is a lightweight Rust↔C++ interop library. Rust calls C++ functions
**directly by their mangled symbol name**, with **no code generation on the C++ side**.

- No generated C++ shims — write and compile C++ freely with your own toolchain.
- MSVC- and Visual-Studio-friendly, including debug builds.
- You declare the C++ functions once in a Rust `extern "C++"` block; the `#[bridge]`
  macro emits the `extern "C"` declarations (with the correct mangled `#[link_name]`)
  and safe Rust wrappers.

Crate version: `0.2.0`. License: MIT OR Apache-2.0.

---

## TL;DR (mental model)

1. You write a Rust `extern "C++" { ... }` block and annotate it with `#[directcpp::bridge]`.
2. For each `pub fn`, the macro:
   - computes the C++ mangled name (Itanium ABI on Linux/macOS, MSVC ABI on Windows),
   - emits `extern "C" { #[link_name="<mangled>"] fn ffi__<name>(...); }`,
   - emits a safe `pub fn <name>(...)` wrapper that marshals arguments and return values.
3. You implement the function in plain C++ (a normal, non-`extern "C"` function).
4. The linker resolves the Rust call directly to your C++ symbol. That is why it is
   called *directcpp*.

`extern "C"` is **NOT** used on the C++ side — Rust links against the real mangled
C++ symbol.

---

## Requirements & constraints

- **Matching memory layout.** Any struct passed by reference or returned by value must
  have an identical memory layout in Rust and C++. Use `#[repr(C)]` on the Rust side.
  The header `res/rust/rust-spt.h` provides layout-compatible helper types
  (`RustString`, `RustVec<T>`, `rust_refstr_t`, `SharedPtr`/`UniquePtr` support, etc.).
  Include it in your C++ code.
- **Return types need a C++ registration.** Any type used as a *return value* must be
  registered once on the C++ side with `ffi::enable_class<T>()` (or
  `ffi::enable_class_sp<T>()` for `shared_ptr<T>`). Types used only as *arguments* do
  **not** need this.
- **Inlined functions must be force-referenced.** If a C++ function you call is inline,
  give it a real symbol via `ffi::force_ref(&func)`. Prefer non-inline functions for interop.
- **Cloneable returns.** A struct returned *by value* is cloned on the Rust side and the
  original C++ object is then destroyed, so the Rust struct must derive `Clone`.

---

## Install

```toml
# Cargo.toml
[dependencies]
directcpp = "0.2.0"
```

Compile your C++ with a build script (`build.rs`) using the `cc` crate, and include
`rust-spt.h`. See `test_proj/` for a complete, runnable example (including a Visual
Studio solution and a `msvcrtd` debug-linking trick).

---

## Quick start

### 1. Simplest function

```rust
#[directcpp::bridge]
extern "C++" {
    pub fn on_start();
}
```

```cpp
// plain C++ — no extern "C"
void on_start() {
    std::cout << "on cpp side start!" << std::endl;
}
```

### 2. Pass a struct by reference, return a struct by value

```rust
#[repr(C)]
struct MagicIn  { ivalue: i32, fvalue: f32, svalue: String }
#[repr(C)]
#[derive(Clone)]                 // returned-by-value structs must be Clone
struct MagicOut { ivalue: i64, fvalue: f64, sa: String, sb: String }

#[directcpp::bridge]
extern "C++" {
    pub fn on_magic(magic: &MagicIn) -> MagicOut;
}
```

```cpp
MagicOut on_magic(const MagicIn& input) {
    MagicOut out;
    out.ivalue = input.ivalue * 2;
    out.fvalue = input.fvalue * 2;
    out.sa = "…";
    out.sb = "…";
    return out;
}

void register_types(void**) {           // call is not required, symbol reference is
    ffi::enable_class<MagicOut>();       // required: MagicOut is a return type
}
```

---

## Type mapping reference

`directcpp` maps a Rust type name to the **same-named** C++ type by default. The
following types are handled specially. "Position" indicates where the mapping applies.

| Rust type                     | Position        | C++ type                         | Notes |
|-------------------------------|-----------------|----------------------------------|-------|
| `i8 i16 i32 i64`              | any             | `int8_t int16_t int int64_t`     | |
| `u8 u16 u32 u64`              | any             | `uint8_t uint16_t uint32_t uint64_t` | |
| `f32 f64`                     | any             | `float double`                   | |
| `bool`                        | any             | `bool`                           | |
| `&CStr`                       | argument        | `const char*`                    | NUL-terminated |
| `&str`                        | argument        | `const char*, size_t`            | expands to **two** C++ params (ptr, len) |
| `&[u8]`                       | argument        | `const uint8_t*, size_t`         | expands to **two** C++ params (ptr, len) |
| `&T` (struct)                 | argument        | `const T&`                       | same layout required |
| `&mut T` (struct)             | argument        | `T&`                             | mutable, use with care |
| `String`                      | argument/return | `RustString`                     | from `rust-spt.h` |
| `Vec<T>`                      | return          | `RustVec<T>`                     | from `rust-spt.h` |
| `&Vec<T>`                     | argument        | `const RustVec<T>&`              | read-only view of a Rust `Vec` |
| `&str` **as a `Vec` element** | element         | `rust_refstr_t`                  | e.g. `&Vec<&str>` → `const RustVec<rust_refstr_t>&` |
| `SharedPtr<T>`                | return          | `std::shared_ptr<T>`             | needs `ffi::enable_class_sp<T>()` |
| `UniquePtr<T>`                | return          | `std::unique_ptr<T>`             | needs `ffi::enable_class<T>()` |
| `SharedPtr<T>` / `UniquePtr<T>` | argument      | `T*`                             | passes the underlying pointer |
| `CPtr<T>`                     | argument        | `T*`                             | opaque C++ pointer (see member functions) |
| `Option<&T>`                  | argument        | `T*`                             | nullable pointer; `None` → `nullptr` |
| `POD<T>`                      | return          | `T`                              | copied by value, **no** destructor called |

A struct returned by value (e.g. `MagicOut`) maps to the C++ type `T` returned by value;
Rust copies + `clone()`s it and then invokes the C++ destructor on the original.

---

## Passing and returning `Vec`

### Return a `Vec<T>` (C++ → Rust)

```rust
#[directcpp::bridge]
extern "C++" {
    pub fn get_bin() -> Vec<u8>;
}
```

```cpp
RustVec<uint8_t> get_bin() {
    uint8_t data[] = {0xba, 0xad, 0xf0, 0x0d};
    return RustVec<uint8_t>(data, sizeof(data));   // move into Rust
}
// register once: ffi::enable_class<RustVec<uint8_t>>();
```

### Pass a `&Vec<T>` (Rust → C++)

A Rust `&Vec<T>` is handed to C++ as a **read-only** `const RustVec<T>&` (a reference,
i.e. a pointer to the Rust-owned buffer). C++ must not mutate or free it.

```rust
#[directcpp::bridge]
extern "C++" {
    pub fn join_strings(parts: &Vec<&str>) -> String;
}

let parts = vec!["alpha", "beta", "gamma"];
let joined = join_strings(&parts);   // "alpha, beta, gamma"
```

```cpp
// &Vec<&str> arrives as const RustVec<rust_refstr_t>& — rust_refstr_t is the
// fat-pointer struct (const char* data; size_t len;) declared in rust-spt.h.
RustString join_strings(const RustVec<rust_refstr_t>& parts) {
    std::string out;
    for (const auto& p : parts) {
        if (!out.empty()) out += ", ";
        out.append(p.data, p.len);
    }
    return RustString(out.data(), out.size());
}
```

Arguments do not require `ffi::enable_class<T>()`; only return types do (here `String`
→ `RustString` needs `ffi::enable_class<RustString>()`).

---

## Attributes and function forms

### `#[namespace(a::b)]` — place a function in a C++ namespace

```rust
#[directcpp::bridge]
extern "C++" {
    #[namespace(myns)]
    pub fn get_message() -> String;
}
```

```cpp
namespace myns {
    RustString get_message() { return "message from c++"; }
}
```

### `#[member_of(Class)]` — call a C++ member function

The macro generates a Rust free function named `Class__method` whose first parameter is a
`CPtr<Class>` (the `this` pointer). Obtain a `CPtr` from a `SharedPtr`/`UniquePtr` via
`.as_cptr()`.

```rust
#[directcpp::bridge]
extern "C++" {
    pub fn make_proof() -> SharedPtr<Proof>;   // get an object

    #[member_of(Proof)]
    pub fn AddString(s: &String);              // -> Proof__AddString(this, s)
    #[member_of(Proof)]
    pub fn Print();                            // -> Proof__Print(this)
}

let obj = make_proof();
Proof__AddString(obj.as_cptr(), &"hello".to_string());
Proof__Print(obj.as_cptr());
```

```cpp
class Proof {
public:
    void AddString(const RustString& s) { /* ... */ }
    void Print() { /* ... */ }
};
```

### `async fn` — asynchronous results

An `async fn` is driven by a C++-side `ValuePromise<T>`. The C++ function receives a
promise pointer and calls `set_value` when the result is ready. The return type must be a
**non-generic** type.

```rust
#[directcpp::bridge]
extern "C++" {
    pub async fn slow_tostr(val: i32) -> String;
}
```

```cpp
void slow_tostr(ValuePromise<RustString>* res, int arg) {
    std::thread([=]{
        RustString sv(("Your number is: " + std::to_string(arg)).c_str());
        res->set_value(sv);
    }).detach();
}
```

### `extern "C++"` vs `extern "C"`

- `extern "C++"` (the usual case): Rust links against the **mangled** C++ symbol.
- `extern "C"`: Rust links against the unmangled symbol name as-is.

### MSVC debug linking

Rust cannot normally link against `msvcrtd.lib`. Add the following once to force it in
debug builds:

```rust
#[directcpp::enable_msvc_debug]
struct Unused;
```

---

## Generating layout-compatible structs

Structs shared across the boundary must have identical layout. The repo includes
`tools/rust2h.py` to generate C++ structs from Rust definitions (e.g. `MagicIn`,
`MagicOut`). It is a convenience helper, **not** a core part of `directcpp`; simple structs
can be translated by hand or with `sed`.

---

## Why not crate `cxx`?

`cxx` is excellent, but:

- It ships **precompiled C++** and does not build cleanly under MSVC debug configuration
  (iterator-debugging ABI mismatch), which breaks Visual Studio debugging.
- It generates a lot of glue code, forcing both sides into a fixed paradigm.

`directcpp` instead exposes a small number of very free interfaces so Rust can call C++
"like a normal library", at the cost of a little manual layout discipline. It aims for
~99% safety, not 100%: once you hold a C++ pointer in Rust, what you do with it is your
responsibility.

---

## Tested platforms

| OS      | Arch     | Toolchain |
|---------|----------|-----------|
| Windows | x86_64   | MSVC      |
| Linux   | x86_64   | GCC/Clang |
| macOS   | x86_64   | Clang     |
| macOS   | aarch64  | Clang     |

---

## Layout of this repository

- `src/` — the `directcpp` runtime crate (`SharedPtr`, `UniquePtr`, `CPtr`, `POD`,
  `FutureValue`, traits).
- `macro/` — the `directcpp-macro` proc-macro crate (`#[bridge]`, `#[enable_msvc_debug]`).
  Uses `syn` to parse the `extern "C++"` block and generate the FFI wrappers + mangled names.
- `res/rust/rust-spt.h` — C++ helper header (`RustString`, `RustVec<T>`, `rust_refstr_t`,
  `ValuePromise`, `ffi::enable_class`, `ffi::force_ref`).
- `test_proj/` — runnable end-to-end example (Rust `main.rs` + C++ `cpp/prove.cpp` +
  Visual Studio solution).
- `tools/rust2h.py` — optional Rust→C++ struct layout generator.
```

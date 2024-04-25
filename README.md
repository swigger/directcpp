# Direct call cpp from Rust!

`directcpp` is yet another method to interop c++ with rust.

It's designed to be lightweight, efficient, MSVC-friendly, no cpp side code generation, work with outer c++ build tools like visual studio.

If you're looking for a way to work with c++ and rust with *Visual Studio*, `directcpp` might be the result.

## why not use crate cxx

Crate `cxx` is a powerful crate for c++/rust interop. However, in my environment visual studio is important, I need it to debug my c++ code. For MSVC, crate `cxx` is not friendly enough. It has pre-compiled c++ code and cannot be compiled in the debug configuration. Specifically, it's because code compiled with the iterator debugging doesn't work together with code compiled without it.

At the same time, I do not want the code generator to make all the code required for interaction. That seems to create a natural barrier between C++ and Rust. I hope that Rust can use C++ code like a normal lib. Of course, it doesn't come without a cost, and I will try to use these interfaces as little as possible. Therefore, I hope that the interfaces that need to be used can be arbitrary and do not need to strictly follow a certain paradigm.

Of course, security is still very important, and I don't want to make too many compromises on the C++ side and make a lot of strange functions for Rust. The same goes for the rust side. Don't convert some structures into a form that can be recognized by c++ in order to call c++ -- though I did that at the beginning -- because it looked quite cumbersome. Some shitty code takes up large chunks of my text, making it difficult for me to read my own code.

Therefore, I chose to generate a small number of very free interfaces so that rust can freely call C++ code with 99% security. I dare not say it is 100%, although crate cxx does this. After all, when you call C++ from Rust, you can cache any pointer you get from C++, which is your freedom, but it is also the price you pay for your freedom.

## Getting started

If you want to get started with `directcpp`, you can start by reading the test code of it. They are in the directory `test_proj` in github where I also demonstrate how to make visual studio and rust coexist peacefully, and debug version compilation is also supported as I used a hacker's method to allow rust to link to the `msvcrtd` debugging library.

You can try the simplest one first:

```rust
use directcpp;

#[directcpp::bridge]
extern "C++" {
	pub fn on_start();
}
```

And in your cpp file, write:

```c++
void on_start(void)
{
	std::cout << "on cpp side start!" << std::endl;
}
```

Note `extern "C"` is **not** needed. Rust would directly call the mangled version of the c++ function. This is why it's called `directcpp`.

You can also pass a reference of a struct into c++ and get another struct back:

```rust
#[directcpp::bridge]
extern "C++" {
	pub fn on_magic(magic: &MagicIn) -> MagicOut;
}
```

To do this , the `MagicIn` and `MagicOut` struct should have exactly the same memory layout between c++ and rust. I provided the `rust_spt.h` in `github` which you can freely include that. This is the price for no code generation on c++ side, and gives the freedom to write and compile c++ code arbitrarily.

You can also get a class object pointer from c++ side and call its member functions while keeping the safety by `SharedPtr`/`std::shared_ptr` in rust/c++, respectively. Check code in [test\_proj](https://github.com/swigger/directcpp/tree/master/test_proj).

You may need to generate some code in your way, thought, as I used a python script to generate some c++ structure from rust, keeping the same memory layout of the them, like `MagicIn` and `MagicOut`. The python code is not a part of `directcpp`, but they with other stuff all work together to make  the final product. So `directcpp` is a loosely organized library which offers the most freedom, but it may take you a little time to get used to it.

If you really like my python version code converter to generate c++ version struct from rust version struct, I can put it on `github` later. However, convert such structures is not hard, you can even do that by some text manipulation tools like`sed`.

## Special Mappings

Normally `directcpp` maps a name in argument as the same named type with C++. However, there are some special types:

* `&CStr` maps to: `const char*` in C++
* `&str` maps to `const char*, size_t` in C++.
* `&[u8]` maps to `const uint8_t*, size_t` in C++.
* `String` maps to `RustString`

## Tested On

* windows with MSVC, x86_64
* Linux, x86_64
* MacOS, x86_64
* MacOS, aarch64


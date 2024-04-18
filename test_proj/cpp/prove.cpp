#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <memory.h>
#include <string>
#include <vector>
#include <iostream>
#include <memory>
#include <thread>
#include <chrono>
#include "rust_spt.h"
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <Windows.h>
#endif
#define extc extern "C"

// this is the two structs that we have in rust side.
// for real world usage, you can generate this from rust side and include it here.
struct MagicIn {
	int ivalue;
	float fvalue;
	RustString svalue;
};

struct MagicOut {
	int64_t ivalue;
	double fvalue;
	RustString sa;
	RustString sb;
};

class Proof
{
	std::string name = "haystack";
	std::vector<std::string> dummy;

public:
	~Proof() {
		fprintf(stderr, "Proof destructed, this = %#llx\n", (long long)this);
	}
	void AddString(const RustString& str) {
		dummy.push_back(str.str());
	}
	void Print();
	void foo() {
		std::cout << "foo " << name << std::endl;
	}
};

void Proof::Print()
{
	for (auto& s : dummy) {
		std::cout << "cxx: got str: " << s << std::endl;
	}
}

void on_start() {
#ifdef _WIN32
	SetConsoleCP(CP_UTF8);
	SetConsoleOutputCP(CP_UTF8);
#endif
	std::cout << "\x1b[1;35mhello from CPP!\x1b[0m" << std::endl;
}

std::shared_ptr<Proof> cpp_ptr(int v) {
	return std::make_shared<Proof>();
}

MagicOut on_magic(const MagicIn& input) {
	MagicOut out;
	std::cout << "c++ received magic in with msg: " << input.svalue.str() << std::endl;
	out.ivalue = input.ivalue * 2;
	out.fvalue = input.fvalue * 2;
	out.sa = "天地玄黄宇宙洪荒";
	out.sb = "😀😁😂😃😄😅😋";
	return out;
}

RustString get_message() {
	return "message from c++";
}

// the only costs in c++ side is to enable some classes and structures for interop.
// only those type used in return values need this. Those used in arguments do not need this.
// you can put the forced references in a separate function that is never called to avoid runtime cost.
void unused_function(volatile void** ptr) {
	ffi::enable_class_sp<Proof>();
	ffi::enable_class<MagicOut>();
	ffi::enable_class<RustString>();

	// if your functions are inlined, you should force reference them to have a function body so that rust can find them.
	// non-inline functions are not required, leave them as is.
	// no need to call: force_ref(&Proof::Print);
	// So, normally you should not use inline(including auto inlined functions in class declaration) functions for interop.
	// I'm just putting them here for an example, not for real world usage.
	ffi::force_ref(&Proof::foo);
	ffi::force_ref(&Proof::AddString);
}

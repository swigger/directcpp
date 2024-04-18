use std::env;
use cc;

fn main() {
	let projname = "test_cpp";
    println!("cargo:rerun-if-changed=cpp/prove.cpp");
    let from_vs = env::var("VisualStudioDir").map(|x| !x.is_empty()).unwrap_or(false);
    let is_debug = env::var("PROFILE").map(|x| x == "debug").unwrap_or(false);
    if from_vs {
        if is_debug {
            println!("cargo:rustc-link-arg-bins=x64/Debug/{}.lib", projname);
            println!("cargo:rerun-if-changed=x64/Debug/{}.lib", projname);
        } else {
            println!("cargo:rustc-link-arg-bins=x64/Release/{}.lib", projname);
            println!("cargo:rerun-if-changed=x64/Release/{}.lib", projname);
        }
    } else {
        let mut cxxb = cc::Build::new();
		cxxb.cpp(true).std("c++20");
        if cfg!(target_os = "windows") {
            env::set_var("VSLANG", "1033");
            cxxb.flag("/EHsc").flag("/utf-8")
                .flag("/D_CRT_SECURE_NO_WARNINGS")
                .flag("/D_CRT_NONSTDC_NO_WARNINGS")
                .flag("/DUNICODE").flag("/D_UNICODE");
        } else {
            cxxb.flag("-Wno-unused-parameter").flag("-Wno-unused-result")
				.flag("-Wno-multichar").flag("-Wno-missing-field-initializers")
				.flag("-g");
        }
        cxxb.file("cpp/prove.cpp").compile(&format!("{}1", projname));
    }
}

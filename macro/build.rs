use std::env;
use cc;

fn main() {
    let mut cco = cc::Build::new();
    cco.cpp(true).std("c++20");
    if cfg!(target_os = "windows") {
        env::set_var("VSLANG", "1033");
        cco.flag("/EHsc").flag("/utf-8")
            .flag("/D_CRT_SECURE_NO_WARNINGS")
            .flag("/D_CRT_NONSTDC_NO_WARNINGS")
            .flag("/DUNICODE")
            .flag("/D_UNICODE");
    } else {
        cco.flag("-Wno-unused-parameter").flag("-Wno-unused-result").flag("-g");
    }
    cco.file("src/bridge.cpp").compile("dxcpp");
    println!("cargo:rerun-if-changed=src/bridge.cpp");
}

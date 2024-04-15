use std::env;
use std::fs;
use cc;

#[allow(dead_code)]
fn dump_env() {
    let env_vars = env::vars();

    // Convert the environment variable pairs into a string
    let env_vars_str = env_vars
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join("\n");

    // Write the environment variables to a file named "env.txt"
    match fs::write("env.txt", env_vars_str) {
        Ok(_) => println!("Environment variables saved to 'env.txt'"),
        Err(err) => eprintln!("Error saving environment variables: {}", err),
    }
}

fn main() {
    println!("cargo:rerun-if-changed=cpp/prove.cpp");
    let from_vs = env::var("VisualStudioDir").map(|x| !x.is_empty()).unwrap_or(false);
    let is_debug = env::var("PROFILE").map(|x| x == "debug").unwrap_or(false);
    if from_vs {
        if is_debug {
            println!("cargo:rustc-link-arg-bins=x64/Debug/test_cpp.lib");
            println!("cargo:rerun-if-changed=x64/Debug/test_cpp.lib");
        } else {
            println!("cargo:rustc-link-arg-bins=x64/Release/test_cpp.lib");
            println!("cargo:rerun-if-changed=x64/Release/test_cpp.lib");
        }
    } else {
        let mut ccb = cc::Build::new();
        if cfg!(target_os = "windows") {
            env::set_var("VSLANG", "1033");
            ccb.flag("/std:c++20").flag("/EHsc").flag("/utf-8")
                .flag("/D_CRT_SECURE_NO_WARNINGS")
                .flag("/D_CRT_NONSTDC_NO_WARNINGS")
                .flag("/DUNICODE")
                .flag("/D_UNICODE");
        } else {
            ccb.flag("-std=c++20").flag("-Wno-unused-parameter").flag("-Wno-unused-result").flag("-g");
        }
        ccb.cpp(true).file("cpp/prove.cpp").compile("test_cpp1");
    }
}

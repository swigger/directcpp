use std::env;
use cc;

/*
fn dump_env() {
    use std::fs::OpenOptions;
    use std::io::Write;
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/env.txt")
    {
        let _ = writeln!(file, "\n========================================");
        let _ = writeln!(file, "--- BUILD.RS TRIGGERED AT: {:?} ---", std::time::SystemTime::now());
        let _ = writeln!(file, "========================================");

        // 1. 打印当前的进程参数 (看看究竟有没有带着特殊的 flag)
        let _ = writeln!(file, "[ARGS]");
        for (i, arg) in env::args().enumerate() {
            let _ = writeln!(file, "  arg[{}]: {}", i, arg);
        }
        let _ = writeln!(file, "");

        // 2. 收集并排序所有的环境变量，方便你比对
        let _ = writeln!(file, "[ENV VARIABLES]");
        let mut env_vars: Vec<(String, String)> = env::vars().collect();
        env_vars.sort_by(|a, b| a.0.cmp(&b.0));

        for (key, value) in env_vars {
            let _ = writeln!(file, "  {}={}", key, value);
        }
        let _ = writeln!(file, "\n========================================\n");
    }
}
*/

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
    // 下面这一段只是为了兼容 RustRover，直接命令运行 cargo test 是不需要的。
    #[cfg(target_os = "macos")]
    if env::var("PROFILE").unwrap_or_default() == "debug" {
        if let Ok(home) = env::var("HOME") {
            let mut rpath = std::path::PathBuf::from(home);
            rpath.push(".rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/lib");
            if rpath.exists() {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", rpath.display());
            }
        }
    }
}

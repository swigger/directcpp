fn main() {
	println!("cargo::metadata=MPATH={}/res", env!("CARGO_MANIFEST_DIR"));
}

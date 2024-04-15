TGTNAME=test_directcpp.exe

all_dbg: target/debug/$(TGTNAME)
all_rls: target/release/$(TGTNAME)

target/debug/$(TGTNAME): src/main.rs
	cargo build

target/release/$(TGTNAME): src/main.rs
	cargo build --release

clean:
	-@rd /s /q target x64

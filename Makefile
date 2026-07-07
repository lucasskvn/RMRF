
build:
	cargo build --release
	cp target/release/rmrf /usr/local/bin/rmrf 2>/dev/null || true

test:
	cargo test --workspace
	cargo clippy --workspace

clean:
	cargo clean

.PHONY: build install test clean

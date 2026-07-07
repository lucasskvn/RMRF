.PHONY: pull build install test clean

pull:
	git pull
	cargo build --release
	cp target/release/rmrf /usr/local/bin/rmrf 2>/dev/null || true
	@echo "Done: rmrf updated and installed"

build:
	cargo build --release

install: build
	cp target/release/rmrf /usr/local/bin/rmrf

test:
	cargo test --workspace
	cargo clippy --workspace

clean:
	cargo clean

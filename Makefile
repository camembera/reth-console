.PHONY: all clean test test-coverage

all:
	cargo build

test:
	cargo test

test-coverage:
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "missing cargo-llvm-cov; install it with: cargo install cargo-llvm-cov"; exit 1; }
	@mkdir -p coverage
	@if command -v rustup >/dev/null 2>&1; then \
		TOOLCHAIN=$$(rustup show active-toolchain | cut -d' ' -f1); \
		rustup run $$TOOLCHAIN cargo llvm-cov --workspace --all-features --summary-only; \
		rustup run $$TOOLCHAIN cargo llvm-cov --workspace --all-features --lcov --output-path coverage/lcov.info; \
	else \
		cargo llvm-cov --workspace --all-features --summary-only; \
		cargo llvm-cov --workspace --all-features --lcov --output-path coverage/lcov.info; \
	fi

clean:
	cargo clean

.PHONY: all fmt fmt-check print-rustfmt-nightly size-check lint test

# Default: the local gate, in the order CI runs it.
default: all

all: fmt-check size-check lint test

# The single source of truth for the rustfmt toolchain. rustfmt.toml enables
# unstable options, so formatting is only reproducible against one exact
# nightly. Bump here and run `make fmt`, committing the reformat alongside it.
RUSTFMT_NIGHTLY ?= nightly-2026-09-21

# Homebrew's cargo shadows rustup's shim, so `cargo fmt` resolves the
# `cargo-fmt`/`rustfmt` binaries off PATH (stable), ignoring the pinned
# toolchain. Pointing RUSTFMT at the pinned toolchain's rustfmt explicitly is
# what makes `rustup run $(RUSTFMT_NIGHTLY) cargo fmt` actually use it.
RUSTFMT = $(shell rustup which --toolchain $(RUSTFMT_NIGHTLY) rustfmt)
export RUSTFMT

print-rustfmt-nightly:
	@echo $(RUSTFMT_NIGHTLY)

fmt:
	# Run twice because rustfmt is not idempotent in one pass under
	# indent_style = "Visual".
	rustup run $(RUSTFMT_NIGHTLY) cargo fmt --all
	rustup run $(RUSTFMT_NIGHTLY) cargo fmt --all

fmt-check:
	rustup run $(RUSTFMT_NIGHTLY) cargo fmt --all --check

# Enforces AGENTS.md's "no .rs file over 500 lines" rule. Excludes
# tests/examples/benches directories (none exist today -- this repo's own
# tests live inline as #[cfg(test)] modules -- but the rule is scoped that
# way if one is ever added) and target/.
MAX_LINES := 500

size-check:
	@violations=$$(find crates -name '*.rs' \
		-not -path '*/target/*' \
		-not -path '*/tests/*' \
		-not -path '*/examples/*' \
		-not -path '*/benches/*' \
		-exec sh -c 'lines=$$(wc -l < "$$1"); if [ "$$lines" -gt $(MAX_LINES) ]; then echo "$$lines $$1"; fi' _ {} \;); \
	if [ -n "$$violations" ]; then \
		echo "Files over $(MAX_LINES) lines:"; \
		echo "$$violations" | sort -rn; \
		exit 1; \
	fi

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

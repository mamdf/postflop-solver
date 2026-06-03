# Default: show available recipes.
default:
	@just --list

# Cores Rayon/the harness may use for AUTOMATED (subagent) test runs, so a
# background agent never pins all cores. Humans/CI use `just test` (full speed).
# Deliberately NOT a global cap (would throttle the server/benches) — see CLAUDE.md.
AGENT_CORES := "4"

# Full test suite at full speed for humans / CI (zstd enables compact-save tests).
test:
	cargo test --features zstd

# Subagent test run: caps Rayon + harness so a background agent stays responsive.
test-agent:
	RAYON_NUM_THREADS={{AGENT_CORES}} RUST_TEST_THREADS=2 cargo test --features zstd

# Single test by name under the agent cap, e.g. `just test-one kuhn`.
test-one NAME:
	RAYON_NUM_THREADS={{AGENT_CORES}} RUST_TEST_THREADS=2 cargo test --features zstd {{NAME}}

# Heavy #[ignore]d solver presets (minutes, several GB RAM). Deliberate only.
test-heavy:
	cargo test --features zstd -- --ignored

# Criterion/custom benches: already release-optimized, LONG, full CPU. Use a timeout.
bench:
	cargo bench

# Build release binary + examples.
build:
	cargo build --release

# Clippy.
clippy:
	cargo clippy --release

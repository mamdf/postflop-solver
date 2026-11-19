# Default: show available recipes.
default:
	@just --list

# Cores Rayon/the harness may use for AUTOMATED (subagent) test runs, so a
# background agent never pins all cores. Humans/CI use `just test` (full speed).
# Deliberately NOT a global cap (would throttle the server/benches) — see CLAUDE.md.
AGENT_CORES := "4"
# Seconds the agent recipes' test run (not the build) may take before it is killed.
AGENT_TIMEOUT := "90"

# Full test suite at full speed for humans / CI (zstd: compact-save tests, profile-io: profile dirs).
test:
	cargo test --features zstd,profile-io

# Subagent test run: caps Rayon + harness so a background agent stays responsive.
test-agent:
	cargo test --features zstd,profile-io --no-run
	RAYON_NUM_THREADS={{AGENT_CORES}} RUST_TEST_THREADS=2 timeout {{AGENT_TIMEOUT}} cargo test --features zstd,profile-io

# Single test by name under the agent cap, e.g. `just test-one kuhn`.
test-one NAME:
	cargo test --features zstd,profile-io --no-run
	RAYON_NUM_THREADS={{AGENT_CORES}} RUST_TEST_THREADS=2 timeout {{AGENT_TIMEOUT}} cargo test --features zstd,profile-io {{NAME}}

# Heavy #[ignore]d solver presets. Run deliberately.
test-heavy:
	cargo test --features zstd -- --ignored

# Criterion/custom benches at full CPU.
bench:
	cargo bench

# Build release binary + examples.
build:
	cargo build --release

# Clippy.
clippy:
	cargo clippy --release

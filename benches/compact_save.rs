//! Reproducible measurements for the compact solution save.
//!
//! Run with:
//!
//! ```text
//! cargo bench --bench compact_save --features zstd
//! ```
//!
//! This is a `harness = false` benchmark: it prints one table per spot instead of using
//! criterion, because the headline numbers are file size / precision (not just timing)
//! and the solver is deterministic, so a single solve per configuration is reproducible.
//!
//! For every spot it solves twice — `allocate_memory(false)` (32-bit float) and
//! `allocate_memory(true)` (16-bit quantized), the only way to vary internal precision —
//! and reports, per storage mode and compression level: on-disk size, save time, load
//! time (River loads include the on-load recompute), estimated RAM at load, and the
//! max per-hand equity/EV error at the root versus the 32-bit reference.

use std::fs;
use std::time::Instant;

use postflop_solver::*;

#[path = "../tests/support/web_case.rs"]
mod web_case;

use web_case::{
    canonical_web_case_spec, snapshot_current_node, CanonicalWebCaseSpec, CurrentNodeSnapshot,
};

const ZSTD_LEVEL: i32 = 12;

fn main() {
    let small = canonical_web_case_spec();
    let medium = CanonicalWebCaseSpec {
        bet_sizes: ("33%, 75%", "60%"),
        effective_stack: 24,
        ..small
    };
    let multi = CanonicalWebCaseSpec {
        bet_sizes: ("33%, 66%, a", "60%, a"),
        effective_stack: 30,
        ..small
    };

    println!("compact_save benchmark (zstd feature: {})\n", cfg!(feature = "zstd"));

    for (name, spec) in [("small", small), ("medium", medium), ("multi-betsize", multi)] {
        run_spot(name, &spec);
    }
}

fn run_spot(name: &str, spec: &CanonicalWebCaseSpec) {
    println!("=== {name} (flop {}, stack {}, bets {:?}) ===", spec.flop, spec.effective_stack, spec.bet_sizes);

    // 32-bit reference: solve once, snapshot the root for the precision comparison.
    let mut reference = build_solved(spec, false);
    reference.back_to_root();
    let reference_root = snapshot_current_node(&mut reference);

    print_header();

    for &compression in &[false, true] {
        let alloc = if compression { "u16" } else { "f32" };
        let mut game = if compression {
            build_solved(spec, true)
        } else {
            build_solved(spec, false)
        };

        for (mode_label, mode, level) in save_configs() {
            let path = std::env::temp_dir().join(format!(
                "pfs_bench_{}_{name}_{alloc}_{mode_label}_{}.bin",
                std::process::id(),
                level.map(|l| l.to_string()).unwrap_or_else(|| "none".to_string())
            ));
            let options = SaveOptions::navigable()
                .with_storage_mode(mode)
                .with_compression(level);

            let (bytes, save_ms) = measure_save(&mut game, &path, &options);
            let (loaded, load_ms) = measure_load(&path);
            let _ = fs::remove_file(&path);

            // RAM and precision only make sense where the whole tree is navigable. For
            // Turn/Flop the loaded `num_storage` fields are still the full-tree counts, so
            // `memory_usage` would overstate the truncated footprint — report it only for
            // River, where the recompute restores the full game.
            let (ram_mb, precision) = if mode == BoardState::River {
                let (unc, comp) = loaded.memory_usage();
                let ram = if compression { comp } else { unc } as f64 / (1024.0 * 1024.0);
                let mut loaded = loaded;
                loaded.back_to_root();
                let root = snapshot_current_node(&mut loaded);
                (Some(ram), Some(root_max_diff(&reference_root, &root)))
            } else {
                (None, None)
            };

            // Predict the on-disk size from storage counts (River only — the predictor
            // models the strategy-only River buffer) and report the error vs. the measured
            // size, validating the estimator.
            let predicted = if mode == BoardState::River {
                let est =
                    estimate_river_save_size(game.storage_counts(), compression, DEFAULT_ZSTD_RATIO);
                Some(if level.is_some() {
                    est.compressed_bytes
                } else {
                    est.uncompressed_bytes
                })
            } else {
                None
            };

            print_row(
                alloc, mode_label, level, bytes, predicted, save_ms, load_ms, ram_mb, precision,
            );
        }
    }
    println!();
}

/// Storage-mode / compression combinations to measure (compressed rows only when the
/// `zstd` feature is enabled).
fn save_configs() -> Vec<(&'static str, BoardState, Option<i32>)> {
    let mut configs = vec![("River", BoardState::River, None)];
    if cfg!(feature = "zstd") {
        configs.push(("River", BoardState::River, Some(ZSTD_LEVEL)));
    }
    configs.push(("Turn", BoardState::Turn, None));
    if cfg!(feature = "zstd") {
        configs.push(("Turn", BoardState::Turn, Some(ZSTD_LEVEL)));
    }
    configs
}

fn build_solved(spec: &CanonicalWebCaseSpec, compression: bool) -> PostFlopGame {
    let bet_sizes = BetSizeOptions::try_from(spec.bet_sizes).unwrap();
    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: spec.starting_pot,
        effective_stack: spec.effective_stack,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
    };
    let card_config = CardConfig {
        range: [spec.oop_range.parse().unwrap(), spec.ip_range.parse().unwrap()],
        flop: flop_from_str(spec.flop).unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };
    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(compression);
    let target_exploitability = spec.starting_pot as f32 * 0.005;
    solve(&mut game, spec.max_iterations, target_exploitability, false);
    game
}

fn measure_save(game: &mut PostFlopGame, path: &std::path::Path, options: &SaveOptions) -> (u64, f64) {
    let mut best_ms = f64::MAX;
    for _ in 0..3 {
        let start = Instant::now();
        save_solution(game, path, options).unwrap();
        best_ms = best_ms.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    let bytes = fs::metadata(path).unwrap().len();
    (bytes, best_ms)
}

fn measure_load(path: &std::path::Path) -> (PostFlopGame, f64) {
    let mut best_ms = f64::MAX;
    let mut last = None;
    for _ in 0..3 {
        let start = Instant::now();
        let (game, _memo) = load_solution(path, &LoadOptions::default()).unwrap();
        best_ms = best_ms.min(start.elapsed().as_secs_f64() * 1000.0);
        last = Some(game);
    }
    (last.unwrap(), best_ms)
}

/// Max per-hand absolute difference in equity and EV at the root.
fn root_max_diff(reference: &CurrentNodeSnapshot, other: &CurrentNodeSnapshot) -> (f32, f32) {
    let mut equity = 0.0f32;
    let mut ev = 0.0f32;
    for (r, o) in reference.hands.iter().zip(&other.hands) {
        equity = equity.max((r.equity - o.equity).abs());
        ev = ev.max((r.ev - o.ev).abs());
    }
    (equity, ev)
}

fn print_header() {
    println!(
        "  {:<6} {:<6} {:<6} {:>10} {:>10} {:>6} {:>9} {:>9} {:>9} {:>11} {:>11}",
        "alloc", "mode", "zstd", "bytes", "pred", "err%", "save ms", "load ms", "ram MB",
        "eq maxdiff", "ev maxdiff"
    );
}

#[allow(clippy::too_many_arguments)]
fn print_row(
    alloc: &str,
    mode: &str,
    level: Option<i32>,
    bytes: u64,
    predicted: Option<u64>,
    save_ms: f64,
    load_ms: f64,
    ram_mb: Option<f64>,
    precision: Option<(f32, f32)>,
) {
    let zstd = level.map(|l| format!("L{l}")).unwrap_or_else(|| "no".to_string());
    let (pred, err) = match predicted {
        Some(pred) => (
            pred.to_string(),
            format!("{:+.1}", (pred as f64 - bytes as f64) / bytes as f64 * 100.0),
        ),
        None => ("-".to_string(), "-".to_string()),
    };
    let ram = ram_mb
        .map(|ram| format!("{ram:.2}"))
        .unwrap_or_else(|| "-".to_string());
    let (eq, ev) = match precision {
        Some((eq, ev)) => (format!("{eq:.2e}"), format!("{ev:.2e}")),
        None => ("-".to_string(), "-".to_string()),
    };
    println!(
        "  {:<6} {:<6} {:<6} {:>10} {:>10} {:>6} {:>9.2} {:>9.2} {:>9} {:>11} {:>11}",
        alloc, mode, zstd, bytes, pred, err, save_ms, load_ms, ram, eq, ev
    );
}

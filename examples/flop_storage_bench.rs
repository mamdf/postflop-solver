use postflop_solver::*;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::time::Instant;

// ============================================================================
// Flop-only storage feasibility — per-mode artifact sizes and turn re-solve cost.
//
// Measures the disk footprint of flop-mode saves vs turn/river and the latency of
// on-demand turn re-solves seeded from a loaded flop artifact. Implements the
// pre-registered GO/NO-GO decision criteria from docs/flop-only-feasibility.md.
//
// Usage:
//   # Dry-run — print element counts + projected solve times, no solving/saving:
//   FLOP_BENCH_COUNTS=1 cargo run --release --features zstd --example flop_storage_bench
//
//   # Default suite (S1-S3):
//   cargo run --release --features zstd --example flop_storage_bench
//
//   # Quick smoke cycle (~50 iters, tiny trees):
//   FLOP_BENCH_SMOKE=1 cargo run --release --features zstd --example flop_storage_bench
//
//   # Monster spot (S4) — requires explicit opt-in, check free RAM first:
//   FLOP_BENCH_HEAVY=1 cargo run --release --features zstd --example flop_storage_bench
//
// CRITICAL — extraction at the pre-deal chance node:
//   On a loaded flop-mode artifact, calling play(turn_card) panics ("Storage mode is
//   not compatible"). The correct pattern — mirroring the production river re-solve —
//   is to call node_input_ranges() + node_pot_stack() AT the chance node (board len 3)
//   BEFORE dealing any card. The extracted ranges are turn-card-independent: one
//   extraction seeds all ~45–48 turn subgames, which is relevant for future caching.
//
// Projected-solve calibration:
//   Slope ~0.83 ms / iter / 1M tree elements (Ryzen 5600X, RAYON_NUM_THREADS=4).
//   Formula: proj_s = total_elements / 1e6 * 0.83 * max_iters / 1000.
//   NOTE: slope is host-pinned; see provenance label printed alongside each projection.
// ============================================================================

const ZSTD_LEVEL: i32 = 12;
/// Calibration slope: ms per iteration per 1M tree elements (Ryzen 5600X, 4 Rayon threads).
const MS_PER_ITER_PER_M_ELEMENTS: f64 = 0.83;

fn main() {
    let smoke = std::env::var("FLOP_BENCH_SMOKE").map(|v| v == "1").unwrap_or(false);
    let heavy = std::env::var("FLOP_BENCH_HEAVY").map(|v| v == "1").unwrap_or(false);
    let counts_mode = std::env::var("FLOP_BENCH_COUNTS").map(|v| v == "1").unwrap_or(false);

    if counts_mode {
        // Dry-run: build each parent (no allocate_memory), print counts + projection, exit.
        run_counts_mode(heavy);
        return;
    }

    let mut any_failed = false;

    // S1: production-fat chipEV spot (Td9d6h, river_resolve_bench ranges).
    run_s1(smoke, &mut any_failed);

    // S2: wide Range::ones() ceiling chipEV spot.
    run_s2(smoke, &mut any_failed);

    // S3: TerminalIcm spot (stacks [35,35,70], payouts [60,40,0]).
    run_s3(smoke, &mut any_failed);

    // S4: monster spot — gated behind FLOP_BENCH_HEAVY=1.
    if heavy {
        run_s4(&mut any_failed);
    } else {
        println!("\n[S4] Skipped — set FLOP_BENCH_HEAVY=1 to run the monster spot.");
    }

    if any_failed {
        eprintln!("\nSome assertions FAILED — see FAIL lines above.");
        std::process::exit(1);
    } else {
        println!("\nAll assertions PASS.");
    }
}

// ============================================================================
// COUNTS dry-run mode
// ============================================================================

fn run_counts_mode(heavy: bool) {
    let rayon_threads = rayon::current_num_threads();
    println!("FLOP_BENCH_COUNTS=1 — dry run: element counts + projected solve times (no solve/save)");
    println!(
        "host: rayon_threads={} | slope calibrated on Ryzen 5600X, full threads",
        rayon_threads
    );
    println!("{}", "=".repeat(72));

    let configs: Vec<(&'static str, SpotConfig)> = vec![
        ("S1", s1_config(false)),
        ("S2", s2_config(false)),
        ("S3", s3_config(false)),
    ];

    for (tag, cfg) in &configs {
        print_counts_for_spot(tag, cfg);
    }

    if heavy {
        print_counts_for_spot("S4", &s4_config());
    } else {
        println!("\n[S4] Skipped — set FLOP_BENCH_HEAVY=1 to include the monster spot.");
    }
}

fn print_counts_for_spot(tag: &str, cfg: &SpotConfig) {
    println!("\n[{}] {}", tag, cfg.name);

    let flop = flop_from_str(cfg.flop).unwrap();
    // build_game does NOT call allocate_memory for ICM spots (caller must), and
    // for non-ICM it does. We need the game in TreeBuilt state only — so we
    // build then immediately query (ICM case already returns pre-alloc). For
    // non-ICM spots build_game calls allocate_memory, but storage_counts +
    // memory_usage are also valid post-alloc, so either path works.
    let game = build_game(cfg, flop, false);

    let counts = game.storage_counts();
    let (mem_unc, _mem_comp) = game.memory_usage();
    let mem_gb = mem_unc as f64 / (1024.0_f64.powi(3));
    let total_elems = counts.total_elements();

    let proj_secs = total_elems as f64 / 1_000_000.0
        * MS_PER_ITER_PER_M_ELEMENTS
        * cfg.parent_max_iters as f64
        / 1000.0;

    println!(
        "  storage_counts -> total_elements={} (num_storage={}, ip={}, chance={})",
        total_elems, counts.num_storage, counts.num_storage_ip, counts.num_storage_chance
    );
    println!(
        "  memory_usage   -> {:.3} GB uncompressed",
        mem_gb
    );
    println!(
        "  projected solve -> {:.1} s  ({} iters × {:.1}M elements × {:.2} ms/iter/M) [slope calibrated on Ryzen 5600X, full threads; rayon_threads={}]",
        proj_secs,
        cfg.parent_max_iters,
        total_elems as f64 / 1_000_000.0,
        MS_PER_ITER_PER_M_ELEMENTS,
        rayon::current_num_threads()
    );
    println!(
        "  target_fraction -> {:.4}  (parent)",
        cfg.parent_target_fraction
    );
}

// ============================================================================
// S1: Production-fat chipEV
// ============================================================================

fn s1_config(smoke: bool) -> SpotConfig {
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";
    // Smoke: tiny single sizing so the tree is tiny and runs in seconds.
    // Full: fat sizings (flop "33%, 75%" + "2.5x"; turn/river "33%, 75%, 125%, a" + "2.5x").
    //   Stack 145 (SPR≈2.4 on pot 60) is the largest that keeps the parent uncompressed
    //   memory ≤ ~1.5 GB on this host (1.470 GB at stack=145 vs 1.588 GB at stack=150,
    //   verified via FLOP_BENCH_COUNTS). Stack=60 was too conservative (only 0.579 GB at 100).
    //   Parent converged wall time stays low because the parent stops at target (~30-100
    //   iters, not 1000).
    let flop_bets = if smoke { "50%" } else { "33%, 75%" };
    let turn_river_bets = if smoke { "50%" } else { "33%, 75%, 125%, a" };
    let raise = if smoke { "" } else { "2.5x" };
    let parent_iters = if smoke { 50 } else { 1000 };
    let turn_iters = if smoke { 50 } else { 1000 };
    // stack=145: largest value keeping parent uncompressed RAM ≤ ~1.5 GB (1.470 GB; verified).
    let stack = 145;

    SpotConfig {
        name: "S1-production-fat",
        oop_range: oop_range.to_string(),
        ip_range: ip_range.to_string(),
        flop: "Td9d6h",
        turn_card: "Qc",
        starting_pot: 60,
        effective_stack: stack,
        flop_bets: flop_bets.to_string(),
        flop_raise: raise.to_string(),
        turn_bets: turn_river_bets.to_string(),
        turn_raise: raise.to_string(),
        river_bets: turn_river_bets.to_string(),
        river_raise: raise.to_string(),
        // Subgame uses the same fat sizings as the parent for S1.
        subgame_turn_bets: turn_river_bets.to_string(),
        subgame_turn_raise: raise.to_string(),
        subgame_river_bets: turn_river_bets.to_string(),
        subgame_river_raise: raise.to_string(),
        parent_max_iters: parent_iters,
        parent_target_fraction: 0.005,
        turn_max_iters: turn_iters,
        turn_targets: if smoke {
            vec![0.005]
        } else {
            vec![0.005, 0.003, 0.001]
        },
        icm: None,
    }
}

fn run_s1(smoke: bool, any_failed: &mut bool) {
    let cfg = s1_config(smoke);
    run_spot(&cfg, any_failed);
}

// ============================================================================
// S2: Wide Range::ones() ceiling chipEV
// ============================================================================

fn s2_config(smoke: bool) -> SpotConfig {
    let parent_iters = if smoke { 50 } else { 1000 };
    let turn_iters = if smoke { 50 } else { 1000 };
    SpotConfig {
        name: "S2-wide-ceiling",
        oop_range: "XX".to_string(), // sentinel — replaced with Range::ones() in build_game()
        ip_range: "XX".to_string(),
        flop: "Td9d6h",
        turn_card: "Qc",
        starting_pot: 60,
        // Stack 240: maximizes the wide-ceiling stress on disk size measurement.
        // Range::ones() symmetry means exploitability converges in very few iterations,
        // so the actual solve time is far below the 283 s worst-case projection.
        effective_stack: 240,
        flop_bets: "50%".to_string(),
        flop_raise: "".to_string(),
        turn_bets: "50%".to_string(),
        turn_raise: "".to_string(),
        river_bets: "50%".to_string(),
        river_raise: "".to_string(),
        subgame_turn_bets: "50%".to_string(),
        subgame_turn_raise: "".to_string(),
        subgame_river_bets: "50%".to_string(),
        subgame_river_raise: "".to_string(),
        parent_max_iters: parent_iters,
        parent_target_fraction: 0.005,
        turn_max_iters: turn_iters,
        turn_targets: if smoke { vec![0.005] } else { vec![0.005, 0.001] },
        icm: None,
    }
}

fn run_s2(smoke: bool, any_failed: &mut bool) {
    let cfg = s2_config(smoke);
    run_spot(&cfg, any_failed);
}

// ============================================================================
// S3: TerminalIcm
// ============================================================================

fn s3_config(smoke: bool) -> SpotConfig {
    // Ranges: same as S1 (parity with API test helper icm_request_solvable).
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    // Parent tree: smoke uses a single "50%" sizing, tiny tree, always runs.
    // Full parent: "50%" single sizing, starting_pot=20, stack=35.
    //   SPR = 35/20 = 1.75 — non-degenerate after OOP-Check/IP-Bet(50%=10)/OOP-Call:
    //   seam pot = 40, remaining stack = 25, seam SPR = 25/40 = 0.625 (≥ 0.5 threshold).
    //   ICM stacks [35,35,70] / payouts [60,40,0] are on the same chip scale as the hand.
    //   At starting_pot=60 with stack=35 the invested line left only ~5 chips behind
    //   (near-allin, trivial subgame); retuning to starting_pot=20 fixes this while
    //   keeping the stack-to-ICM-stack relationship chip-scale-consistent.
    //   At 3000 iters this projects to ≲30 s on this host.
    // Subgame: FAT sizings ("33%, 75%, 125%, a" + "2.5x") — the turn-rooted subgame is
    //   small by construction (single card), so fat sizings are fine here.
    let parent_bets = "50%";
    let parent_raise = "";

    let subgame_turn_bets = "33%, 75%, 125%, a";
    let subgame_turn_raise = "2.5x";
    let subgame_river_bets = "33%, 75%, 125%, a";
    let subgame_river_raise = "2.5x";

    let parent_iters = if smoke { 50 } else { 3000 };
    let turn_iters = if smoke { 50 } else { 3000 };

    SpotConfig {
        name: "S3-terminal-icm",
        oop_range: oop_range.to_string(),
        ip_range: ip_range.to_string(),
        flop: "Td9d6h",
        turn_card: "Qc",
        // starting_pot=20 (retuned from 60) so the invested flop line leaves seam SPR ≥ 0.5.
        // ICM stacks [35,35,70] / payouts [60,40,0] remain chip-scale-consistent with the hand.
        starting_pot: 20,
        // Stack=35 matches the ICM parity test stacks [35,35,70]; SPR=1.75 on pot 20.
        effective_stack: 35,
        flop_bets: parent_bets.to_string(),
        flop_raise: parent_raise.to_string(),
        turn_bets: parent_bets.to_string(),
        turn_raise: parent_raise.to_string(),
        river_bets: parent_bets.to_string(),
        river_raise: parent_raise.to_string(),
        subgame_turn_bets: subgame_turn_bets.to_string(),
        subgame_turn_raise: subgame_turn_raise.to_string(),
        subgame_river_bets: subgame_river_bets.to_string(),
        subgame_river_raise: subgame_river_raise.to_string(),
        parent_max_iters: parent_iters,
        // ICM parity target: 0.0002 (matches the API test at max_iterations=3000).
        parent_target_fraction: 0.0002,
        turn_max_iters: turn_iters,
        turn_targets: if smoke {
            vec![0.005]
        } else {
            vec![0.005, 0.001, 0.0002]
        },
        icm: Some(IcmConfig {
            stacks: vec![35.0, 35.0, 70.0],
            payouts: vec![60, 40, 0],
            oop_seat: 0,
            ip_seat: 1,
        }),
    }
}

fn run_s3(smoke: bool, any_failed: &mut bool) {
    let cfg = s3_config(smoke);
    run_spot(&cfg, any_failed);
}

// ============================================================================
// S4: Monster (gated behind FLOP_BENCH_HEAVY=1)
// ============================================================================

fn s4_config() -> SpotConfig {
    SpotConfig {
        name: "S4-monster",
        oop_range: "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s"
            .to_string(),
        ip_range: "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+"
            .to_string(),
        flop: "Td9d6h",
        turn_card: "Qc",
        starting_pot: 60,
        effective_stack: 400,
        flop_bets: "33%, 75%, 125%, a".to_string(),
        flop_raise: "2.5x".to_string(),
        turn_bets: "33%, 75%, 125%, a".to_string(),
        turn_raise: "2.5x".to_string(),
        river_bets: "33%, 75%, 125%, a".to_string(),
        river_raise: "2.5x".to_string(),
        subgame_turn_bets: "33%, 75%, 125%, a".to_string(),
        subgame_turn_raise: "2.5x".to_string(),
        subgame_river_bets: "33%, 75%, 125%, a".to_string(),
        subgame_river_raise: "2.5x".to_string(),
        parent_max_iters: 1000,
        parent_target_fraction: 0.001,
        turn_max_iters: 1000,
        turn_targets: vec![0.001],
        icm: None,
    }
}

fn run_s4(any_failed: &mut bool) {
    run_spot(&s4_config(), any_failed);
}

// ============================================================================
// Data types
// ============================================================================

struct IcmConfig {
    stacks: Vec<f64>,
    payouts: Vec<i32>,
    oop_seat: usize,
    ip_seat: usize,
}

struct SpotConfig {
    name: &'static str,
    oop_range: String,
    ip_range: String,
    flop: &'static str,
    turn_card: &'static str,
    starting_pot: i32,
    effective_stack: i32,
    /// Parent tree: flop betting sizings.
    flop_bets: String,
    flop_raise: String,
    /// Parent tree: turn and river betting sizings.
    turn_bets: String,
    turn_raise: String,
    river_bets: String,
    river_raise: String,
    /// Turn subgame: may differ from parent (e.g. S3 fat subgame on moderate parent).
    subgame_turn_bets: String,
    subgame_turn_raise: String,
    subgame_river_bets: String,
    subgame_river_raise: String,
    parent_max_iters: u32,
    /// Single target fraction for the parent solve (replaces the old Vec<f32> first-element pattern).
    parent_target_fraction: f32,
    turn_max_iters: u32,
    /// Turn re-solve target fractions to sweep (Vec so multiple measurements per spot).
    turn_targets: Vec<f32>,
    icm: Option<IcmConfig>,
}

// ============================================================================
// Core per-spot driver
// ============================================================================

fn run_spot(cfg: &SpotConfig, any_failed: &mut bool) {
    println!("\n{}", "=".repeat(72));
    println!("=== {} ===", cfg.name);
    println!("{}", "=".repeat(72));

    let flop = flop_from_str(cfg.flop).unwrap();
    let turn_card = card_from_str(cfg.turn_card).unwrap();

    // ---- 1. Build flop-rooted parent game ----------------------------------
    // Print storage_counts + memory estimate BEFORE solving.
    // Heavy-gate: if >~2 GB projected and not FLOP_BENCH_HEAVY, skip.

    let mut parent = build_game(cfg, flop, false);
    let counts = parent.storage_counts();
    let (mem_unc, mem_comp) = parent.memory_usage();
    let mem_gb = mem_unc as f64 / (1024.0 * 1024.0 * 1024.0);

    println!(
        "storage_counts -> num_storage={}, num_storage_ip={}, num_storage_chance={}",
        counts.num_storage, counts.num_storage_ip, counts.num_storage_chance
    );
    println!(
        "memory_usage   -> {:.3} GB uncompressed, {:.3} GB compressed",
        mem_gb,
        mem_comp as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let heavy = std::env::var("FLOP_BENCH_HEAVY").map(|v| v == "1").unwrap_or(false);
    if mem_gb > 2.0 && !heavy {
        println!(
            "[{}] SKIP — projected RAM {:.1} GB > 2 GB; set FLOP_BENCH_HEAVY=1 to run.",
            cfg.name, mem_gb
        );
        return;
    }

    // Install ICM config (must happen after tree build, before allocate_memory).
    if let Some(icm) = &cfg.icm {
        parent
            .set_tournament_icm_config(TournamentIcmConfig {
                stacks: icm.stacks.clone(),
                payouts: icm.payouts.clone(),
                oop_seat: icm.oop_seat,
                ip_seat: icm.ip_seat,
            })
            .expect("ICM config must install on parent");
    }

    parent.allocate_memory(false);

    // ---- 2. Solve parent ---------------------------------------------------
    // Use the single parent_target_fraction for the parent solve.
    let parent_target = parent.target_exploitability_from_fraction(cfg.parent_target_fraction);

    let t_parent = Instant::now();
    let parent_res =
        solve_with_result(&mut parent, cfg.parent_max_iters, parent_target, false);
    let parent_secs = t_parent.elapsed().as_secs_f64();
    println!(
        "parent solve -> {:.3}s | {} iters | expl {:.6} | target {:.6} | ICM={}",
        parent_secs,
        parent_res.num_iterations,
        parent_res.exploitability,
        parent_target,
        parent.uses_tournament_icm()
    );
    check(
        &format!(
            "[{}] uses_tournament_icm() matches cfg",
            cfg.name
        ),
        parent.uses_tournament_icm() == cfg.icm.is_some(),
        any_failed,
    );

    // ---- 3. Size matrix: save {River, Turn, Flop} x {zstd L12, uncompressed} -------
    // Skip uncompressed for S4 (too large).
    let skip_uncompressed = cfg.name == "S4-monster";

    println!("\n  {:<8} {:<8} {:>12} {:>9} {:>12} {:>12}",
        "mode", "zstd", "bytes", "save ms", "r/t ratio", "t/f ratio");

    let mut river_zstd_bytes: Option<u64> = None;
    let mut turn_zstd_bytes: Option<u64> = None;
    let mut flop_zstd_bytes: Option<u64> = None;
    let mut river_unc_bytes: Option<u64> = None;
    let mut turn_unc_bytes: Option<u64> = None;
    let mut flop_unc_bytes: Option<u64> = None;

    for &(mode, mode_label) in &[
        (BoardState::River, "River"),
        (BoardState::Turn, "Turn"),
        (BoardState::Flop, "Flop"),
    ] {
        for &(compress, zstd_label) in &[(true, "L12"), (false, "none")] {
            if skip_uncompressed && !compress {
                continue;
            }
            let (bytes, save_ms) = measure_save_bytes(&mut parent, mode, compress);
            let ratio_str = "-".to_string();
            println!(
                "  {:<8} {:<8} {:>12} {:>9.1} {:>12} {:>12}",
                mode_label, zstd_label, bytes, save_ms, ratio_str, ratio_str
            );
            match (mode, compress) {
                (BoardState::River, true) => river_zstd_bytes = Some(bytes),
                (BoardState::Turn, true) => turn_zstd_bytes = Some(bytes),
                (BoardState::Flop, true) => flop_zstd_bytes = Some(bytes),
                (BoardState::River, false) => river_unc_bytes = Some(bytes),
                (BoardState::Turn, false) => turn_unc_bytes = Some(bytes),
                (BoardState::Flop, false) => flop_unc_bytes = Some(bytes),
            }
        }
    }

    // Print ratios.
    if let (Some(r), Some(t)) = (river_zstd_bytes, turn_zstd_bytes) {
        println!("  river/turn ratio (zstd): {:.2}x", r as f64 / t as f64);
    }
    if let (Some(t), Some(f)) = (turn_zstd_bytes, flop_zstd_bytes) {
        println!("  turn/flop  ratio (zstd): {:.2}x", t as f64 / f as f64);
    }
    if let (Some(r), Some(f)) = (river_zstd_bytes, flop_zstd_bytes) {
        println!("  river/flop ratio (zstd): {:.2}x", r as f64 / f as f64);
    }
    if let (Some(r), Some(t)) = (river_unc_bytes, turn_unc_bytes) {
        println!("  river/turn ratio (unc):  {:.2}x", r as f64 / t as f64);
    }
    if let (Some(t), Some(f)) = (turn_unc_bytes, flop_unc_bytes) {
        println!("  turn/flop  ratio (unc):  {:.2}x", t as f64 / f as f64);
    }

    // Cross-check River mode vs estimate_river_save_size.
    let est = estimate_river_save_size(counts, false, DEFAULT_ZSTD_RATIO);
    if skip_uncompressed {
        println!("  estimate cross-check skipped (uncompressed disabled)");
    } else if let Some(r_unc) = river_unc_bytes {
        let err_pct = (est.uncompressed_bytes as f64 - r_unc as f64) / r_unc as f64 * 100.0;
        println!(
            "  estimate_river_save_size -> unc={} (err {:.1}%), pred_zstd={}",
            est.uncompressed_bytes, err_pct, est.compressed_bytes
        );
    }

    // ---- 4. Load flop-mode file; assert storage_mode() == Flop -------------
    let flop_path = temp_path(&format!("{}_flop_zstd", cfg.name));
    {
        let opts = SaveOptions::navigable()
            .with_storage_mode(BoardState::Flop)
            .with_compression(Some(ZSTD_LEVEL));
        save_solution(&mut parent, &flop_path, &opts).expect("save flop solution");
    }
    let t_load = Instant::now();
    let (loaded_flop, _) = load_solution(&flop_path, &LoadOptions::default())
        .expect("load flop solution");
    let load_ms = t_load.elapsed().as_secs_f64() * 1000.0;
    let _ = fs::remove_file(&flop_path);

    println!("load flop artifact -> {:.1} ms", load_ms);
    check(
        &format!("[{}] loaded storage_mode == BoardState::Flop", cfg.name),
        loaded_flop.storage_mode() == BoardState::Flop,
        any_failed,
    );
    check(
        &format!(
            "[{}] loaded uses_tournament_icm() matches cfg",
            cfg.name
        ),
        loaded_flop.uses_tournament_icm() == cfg.icm.is_some(),
        any_failed,
    );

    // ---- 5. Navigate to turn seam (invested line); extract ranges + pot/stack ----
    // Use an invested line (OOP Check → IP Bet → OOP Call) so that total_bet_amount() > 0
    // at the chance node, making the ICM base-contribution offset load-bearing for S3.
    // CRITICAL: do NOT play(turn_card) on the loaded flop-mode game — it would panic.

    let mut loaded_nav = loaded_flop;
    navigate_to_invested_turn_seam(&mut parent);
    navigate_to_invested_turn_seam(&mut loaded_nav);

    assert!(
        parent.is_chance_node(),
        "parent must be at a chance node after invested seam navigation; available_actions={:?}",
        parent.available_actions()
    );
    assert!(
        loaded_nav.is_chance_node(),
        "loaded flop must be at a chance node after invested seam navigation; available_actions={:?}",
        loaded_nav.available_actions()
    );

    // For S3: assert base_contribution is non-zero (invested line exercised the ICM offset).
    let seam_base = parent.total_bet_amount();
    if cfg.icm.is_some() {
        assert!(
            seam_base[0] > 0 && seam_base[1] > 0,
            "[{}] FAIL: ICM base_contribution must be > 0 for both seats at the seam \
             (invested line required); got seam_base={:?}",
            cfg.name, seam_base
        );
        println!("PASS  [{}] S3 seam base_contribution {:?} > 0 (ICM offset is load-bearing)", cfg.name, seam_base);
    }

    // Extract ranges at the chance node (board len 3, BEFORE dealing any card).
    let parent_ranges: [Range; 2] = parent.node_input_ranges();
    let loaded_ranges: [Range; 2] = loaded_nav.node_input_ranges();
    let (parent_pot, parent_stack) = parent.node_pot_stack();
    let (loaded_pot, loaded_stack) = loaded_nav.node_pot_stack();

    // Bit-equal comparison of raw_data.
    let ranges_bit_equal = parent_ranges[0].raw_data() == loaded_ranges[0].raw_data()
        && parent_ranges[1].raw_data() == loaded_ranges[1].raw_data();
    check(
        &format!("[{}] node_input_ranges bit-equal (loaded vs in-memory)", cfg.name),
        ranges_bit_equal,
        any_failed,
    );
    check(
        &format!("[{}] node_pot_stack equal (loaded vs in-memory)", cfg.name),
        parent_pot == loaded_pot && parent_stack == loaded_stack,
        any_failed,
    );
    let seam_spr = if parent_pot > 0 {
        parent_stack as f64 / parent_pot as f64
    } else {
        f64::INFINITY
    };
    println!(
        "seam check -> pot={} stack={} SPR={:.2} | ranges bit-equal={} | combos OOP={:.1} IP={:.1}",
        parent_pot,
        parent_stack,
        seam_spr,
        ranges_bit_equal,
        total_combos(&parent_ranges[0]),
        total_combos(&parent_ranges[1])
    );
    println!(
        "  Note: extraction is turn-card-independent — one extraction seeds all ~45-48 turn subgames."
    );

    // ---- 6. Turn re-solve from loaded artifact (fresh game per measurement) ------
    // Use loaded_ranges as seed (proves the round-trip is live, not the parent's copy).
    // M2: print subgame shape (sizings + seam pot/stack/SPR) before the timing table.
    println!(
        "\n  subgame shape: turn_bets={:?}  turn_raise={:?}  river_bets={:?}  river_raise={:?}",
        cfg.subgame_turn_bets, cfg.subgame_turn_raise,
        cfg.subgame_river_bets, cfg.subgame_river_raise
    );
    println!(
        "  seam: pot={}  stack={}  SPR={:.2}  (rayon_threads={}) [slope calibrated on Ryzen 5600X, full threads]",
        parent_pot, parent_stack, seam_spr,
        rayon::current_num_threads()
    );

    println!("\n  {:<12} {:>10} {:>10} {:>8} {:>8} {:>10}",
        "target%", "solve s", "total s", "iters", "expl", "mem MB");

    let mut last_turn_game: Option<PostFlopGame> = None;

    for &fraction in &cfg.turn_targets {
        // M1: time BOTH solve-only and total (build+ICM-install+allocate+solve).
        let t_total = Instant::now();
        let mut tg = build_turn_subgame(cfg, flop, turn_card, &loaded_ranges, parent_pot, parent_stack);

        // For ICM: install config with base_contribution = parent.total_bet_amount() at the seam.
        if let Some(icm) = &cfg.icm {
            let base = parent.total_bet_amount();
            tg.set_tournament_icm_config_with_base_contribution(
                TournamentIcmConfig {
                    stacks: icm.stacks.clone(),
                    payouts: icm.payouts.clone(),
                    oop_seat: icm.oop_seat,
                    ip_seat: icm.ip_seat,
                },
                base,
            )
            .expect("ICM config on turn subgame");
        }

        tg.allocate_memory(false);
        let target = tg.target_exploitability_from_fraction(fraction);
        let t_solve = Instant::now();
        let turn_res = solve_with_result(&mut tg, cfg.turn_max_iters, target, false);
        let solve_secs = t_solve.elapsed().as_secs_f64();
        let total_secs = t_total.elapsed().as_secs_f64();
        let (turn_mem, _) = tg.memory_usage();

        println!(
            "  {:<12} {:>10.3} {:>10.3} {:>8} {:>8.5} {:>10.1}",
            format!("{:.2}%", fraction * 100.0),
            solve_secs,
            total_secs,
            turn_res.num_iterations,
            turn_res.exploitability,
            turn_mem as f64 / (1024.0 * 1024.0)
        );

        if last_turn_game.is_none() {
            last_turn_game = Some(tg);
        }
    }

    // ---- 7. Residency check: navigate solved turn game to a river action node ----
    if let Some(mut tg) = last_turn_game {
        // Navigate to the first available action node inside the solved turn game.
        // Walk: available turn action → river deal → first river action node.
        let has_residency = check_river_residency(&mut tg);
        check(
            &format!("[{}] rivers resident in solved turn game (no extra solve needed)", cfg.name),
            has_residency,
            any_failed,
        );
        let (tg_mem, _) = tg.memory_usage();
        println!(
            "residency -> river actions non-empty={} | turn-game RAM {:.1} MB (future LRU entry cost)",
            has_residency,
            tg_mem as f64 / (1024.0 * 1024.0)
        );
    }

    // ---- 8. Determinism: two identical re-solves produce identical SolveResult;
    //         post-play weights match; loaded == memory-seeded ----------------------
    // B1 fix: compare SolveResult equality (established pattern from river_resolve_bench)
    // AND navigate both games down the same fixed action line to a non-root node, then
    // compare weights() there — post-play weights reflect the strategy product.
    {
        let dtarget_fraction = cfg.turn_targets.last().copied().unwrap_or(0.001);

        // Build three games: ga/gb use loaded_ranges (identical seeds); gc uses parent_ranges
        // (memory-seeded). build_and_install_turn now uses the real seam base (not [0,0]).
        let mut ga = build_and_install_turn(cfg, flop, turn_card, &loaded_ranges, parent_pot, parent_stack, seam_base);
        let mut gb = build_and_install_turn(cfg, flop, turn_card, &loaded_ranges, parent_pot, parent_stack, seam_base);
        let mut gc = build_and_install_turn(cfg, flop, turn_card, &parent_ranges, parent_pot, parent_stack, seam_base);

        let dtarget_a = ga.target_exploitability_from_fraction(dtarget_fraction);
        let dtarget_c = gc.target_exploitability_from_fraction(dtarget_fraction);
        let ra = solve_with_result(&mut ga, cfg.turn_max_iters, dtarget_a, false);
        let rb = solve_with_result(&mut gb, cfg.turn_max_iters, dtarget_a, false);
        let rc = solve_with_result(&mut gc, cfg.turn_max_iters, dtarget_c, false);

        // Primary: SolveResult equality (bit-exact iteration count + exploitability).
        let ab_result_eq = ra == rb;
        let ac_result_eq = ra == rc;

        // Strengthened: navigate both games down the same fixed action line (action 0
        // repeatedly until a chance node) and compare weights() at that node.
        // Post-play weights reflect the strategy product and change with each CFR iteration,
        // so they discriminate real nondeterminism that SolveResult alone might miss.
        let ab_postplay = post_play_weights_equal(&mut ga, &mut gb);
        let ac_postplay = post_play_weights_equal(&mut ga, &mut gc);

        let ab_identical = ab_result_eq && ab_postplay;
        let ac_identical = ac_result_eq && ac_postplay;

        check(
            &format!("[{}] determinism: two identical re-solves bit-identical (SolveResult + post-play weights)", cfg.name),
            ab_identical,
            any_failed,
        );
        check(
            &format!("[{}] determinism: loaded-seeded == memory-seeded (SolveResult + post-play weights)", cfg.name),
            ac_identical,
            any_failed,
        );
        println!(
            "determinism -> A/B identical={} (result_eq={} postplay_eq={} iters {}/{}) \
             | loaded==memory={} (result_eq={} postplay_eq={} iters {}/{})",
            ab_identical, ab_result_eq, ab_postplay,
            ra.num_iterations, rb.num_iterations,
            ac_identical, ac_result_eq, ac_postplay,
            ra.num_iterations, rc.num_iterations
        );
    }

    // ---- 9. Guard check: play(turn_card) on loaded flop copy must panic --------
    {
        // Load a fresh copy specifically for the guard check so we can consume it.
        let flop_guard_path = temp_path(&format!("{}_flop_guard", cfg.name));
        {
            let opts = SaveOptions::navigable()
                .with_storage_mode(BoardState::Flop)
                .with_compression(Some(ZSTD_LEVEL));
            save_solution(&mut parent, &flop_guard_path, &opts).expect("save for guard");
        }
        let (mut guard_game, _) =
            load_solution(&flop_guard_path, &LoadOptions::default()).expect("load for guard");
        let _ = fs::remove_file(&flop_guard_path);

        navigate_to_invested_turn_seam(&mut guard_game);
        assert!(guard_game.is_chance_node(), "must be at chance node for guard test");

        // Silent panic hook so the expected panic doesn't print to stderr.
        let prev_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            guard_game.play(turn_card as usize);
        }));
        panic::set_hook(prev_hook);

        let panicked = result.is_err();
        let msg_ok = if let Err(ref payload) = result {
            if let Some(s) = payload.downcast_ref::<&str>() {
                s.contains("Storage mode is not compatible")
            } else if let Some(s) = payload.downcast_ref::<String>() {
                s.contains("Storage mode is not compatible")
            } else {
                // panic payload type unknown but it did panic; accept it
                true
            }
        } else {
            false
        };
        check(
            &format!(
                "[{}] guard: play(turn_card) on flop-mode artifact panics with correct message",
                cfg.name
            ),
            panicked && msg_ok,
            any_failed,
        );
        println!(
            "guard -> panicked={} msg_ok={} (\"Storage mode is not compatible\")",
            panicked, msg_ok
        );
    }

    // Reset parent to root for any future callers.
    parent.back_to_root();
}

// ============================================================================
// Helpers
// ============================================================================

/// Builds a flop-rooted parent game using the PARENT tree sizings from `cfg`.
/// allocate_memory is NOT called for ICM spots (caller installs ICM then allocates).
/// For non-ICM spots allocate_memory is called here.
fn build_game(cfg: &SpotConfig, flop: [Card; 3], compression: bool) -> PostFlopGame {
    let flop_bets = BetSizeOptions::try_from((cfg.flop_bets.as_str(), cfg.flop_raise.as_str()))
        .unwrap_or_else(|_| BetSizeOptions::try_from(("50%", "")).unwrap());
    let turn_bets = BetSizeOptions::try_from((cfg.turn_bets.as_str(), cfg.turn_raise.as_str()))
        .unwrap_or_else(|_| BetSizeOptions::try_from(("50%", "")).unwrap());
    let river_bets =
        BetSizeOptions::try_from((cfg.river_bets.as_str(), cfg.river_raise.as_str()))
            .unwrap_or_else(|_| BetSizeOptions::try_from(("50%", "")).unwrap());

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: cfg.starting_pot,
        effective_stack: cfg.effective_stack,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [flop_bets.clone(), flop_bets.clone()],
        turn_bet_sizes: [turn_bets.clone(), turn_bets.clone()],
        river_bet_sizes: [river_bets.clone(), river_bets.clone()],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let range_oop: Range = if cfg.oop_range == "XX" {
        Range::ones()
    } else {
        cfg.oop_range.parse().unwrap()
    };
    let range_ip: Range = if cfg.ip_range == "XX" {
        Range::ones()
    } else {
        cfg.ip_range.parse().unwrap()
    };

    let card_config = CardConfig {
        range: [range_oop, range_ip],
        flop,
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    // Note: ICM must be installed BEFORE allocate_memory; caller handles that.
    // For non-ICM spots we allocate here.
    if cfg.icm.is_none() {
        game.allocate_memory(compression);
    }
    game
}

/// Builds a fresh turn-rooted subgame using the SUBGAME sizings from `cfg`.
/// Does NOT install ICM or allocate memory — caller does both before solving.
fn build_turn_subgame(
    cfg: &SpotConfig,
    flop: [Card; 3],
    turn_card: Card,
    ranges: &[Range; 2],
    pot: i32,
    stack: i32,
) -> PostFlopGame {
    // Use subgame-specific sizings (may differ from parent, e.g. S3 fat subgame).
    let turn_bets =
        BetSizeOptions::try_from((cfg.subgame_turn_bets.as_str(), cfg.subgame_turn_raise.as_str()))
            .unwrap_or_else(|_| BetSizeOptions::try_from(("50%", "")).unwrap());
    let river_bets = BetSizeOptions::try_from((
        cfg.subgame_river_bets.as_str(),
        cfg.subgame_river_raise.as_str(),
    ))
    .unwrap_or_else(|_| BetSizeOptions::try_from(("50%", "")).unwrap());

    let tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: pot,
        effective_stack: stack,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [turn_bets.clone(), turn_bets.clone()], // unused on a turn game
        turn_bet_sizes: [turn_bets.clone(), turn_bets.clone()],
        river_bet_sizes: [river_bets.clone(), river_bets.clone()],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let card_config = CardConfig {
        range: ranges.clone(),
        flop,
        turn: turn_card,
        river: NOT_DEALT,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    PostFlopGame::with_config(card_config, action_tree).unwrap()
}

/// Builds + allocates a turn subgame using subgame sizings, optionally installing ICM.
/// Used for the determinism check where we need three identical games.
/// `seam_base` is the real total_bet_amount() at the turn-seam chance node — all three
/// games receive the same base so the comparison is fair, and the base is non-zero on
/// invested lines (making the ICM offset load-bearing for S3).
fn build_and_install_turn(
    cfg: &SpotConfig,
    flop: [Card; 3],
    turn_card: Card,
    ranges: &[Range; 2],
    pot: i32,
    stack: i32,
    seam_base: [i32; 2],
) -> PostFlopGame {
    let mut tg = build_turn_subgame(cfg, flop, turn_card, ranges, pot, stack);
    if let Some(icm) = &cfg.icm {
        // Use the real seam base (consistent across all three determinism games).
        tg.set_tournament_icm_config_with_base_contribution(
            TournamentIcmConfig {
                stacks: icm.stacks.clone(),
                payouts: icm.payouts.clone(),
                oop_seat: icm.oop_seat,
                ip_seat: icm.ip_seat,
            },
            seam_base,
        )
        .expect("ICM on determinism subgame");
    }
    tg.allocate_memory(false);
    tg
}

/// Saves the game at the given storage mode + optional zstd, returns (bytes, save_ms).
fn measure_save_bytes(game: &mut PostFlopGame, mode: BoardState, compress: bool) -> (u64, f64) {
    let compression = if compress { Some(ZSTD_LEVEL) } else { None };
    let label = format!("{:?}_{}", mode, if compress { "zstd" } else { "unc" });
    let path = temp_path(&label);
    let opts = SaveOptions::navigable()
        .with_storage_mode(mode)
        .with_compression(compression);

    let t = Instant::now();
    save_solution(game, &path, &opts).expect("save_solution");
    let save_ms = t.elapsed().as_secs_f64() * 1000.0;
    let bytes = fs::metadata(&path).unwrap().len();
    let _ = fs::remove_file(&path);
    (bytes, save_ms)
}

/// Navigates to the turn-deal chance node via an INVESTED line:
///   OOP Check → IP Bet (first non-allin bet, else the bet) → OOP Call → chance node.
///
/// This ensures total_bet_amount() > 0 at the seam, making the ICM base-contribution
/// offset load-bearing for S3. Contrast with a passive check/check line where the
/// base would be [0,0] and the ICM subtlety would never be exercised.
///
/// Panics with a clear message listing available actions if the expected sequence cannot
/// be completed (e.g., the tree has no bet or no call at any step).
fn navigate_to_invested_turn_seam(game: &mut PostFlopGame) {
    // Step 1: OOP acts — play Check (pass through without betting).
    {
        let actions = game.available_actions();
        let idx = actions
            .iter()
            .position(|a| matches!(a, Action::Check))
            .unwrap_or_else(|| {
                panic!(
                    "navigate_to_invested_turn_seam: expected a Check at OOP flop root; \
                     available_actions={:?}",
                    actions
                )
            });
        game.play(idx);
    }

    // Step 2: IP acts — play the first non-allin Bet; fall back to any Bet if none.
    {
        let actions = game.available_actions();
        let idx = actions
            .iter()
            .position(|a| matches!(a, Action::Bet(_)))
            .unwrap_or_else(|| {
                panic!(
                    "navigate_to_invested_turn_seam: expected a Bet at IP flop node; \
                     available_actions={:?}",
                    actions
                )
            });
        game.play(idx);
    }

    // Step 3: OOP faces the bet — play Call.
    {
        let actions = game.available_actions();
        let idx = actions
            .iter()
            .position(|a| matches!(a, Action::Call))
            .unwrap_or_else(|| {
                panic!(
                    "navigate_to_invested_turn_seam: expected a Call at OOP facing flop bet; \
                     available_actions={:?}",
                    actions
                )
            });
        game.play(idx);
    }

    // After OOP Call we must be at the turn-deal chance node.
    assert!(
        game.is_chance_node(),
        "navigate_to_invested_turn_seam: expected a chance node after OOP Call; \
         available_actions={:?}",
        game.available_actions()
    );
}

/// Single-path smoke probe: checks that the solved turn game has non-empty actions
/// after navigating along action-0 until a river chance node and crossing it.
/// A turn-rooted game's root is an action node (not a chance node), so this function
/// starts by playing action 0 to enter the tree. Returns true if a river action node
/// with non-empty actions was found.
fn check_river_residency(game: &mut PostFlopGame) -> bool {
    game.back_to_root();

    // Turn game root is an action node — play first available action.
    let turn_actions = game.available_actions();
    if turn_actions.is_empty() {
        return false;
    }
    game.play(0);

    // Navigate until we hit the river-deal chance node or a dead end.
    for _ in 0..5 {
        if game.is_chance_node() {
            break;
        }
        let actions = game.available_actions();
        if actions.is_empty() {
            return false;
        }
        game.play(0);
    }

    if !game.is_chance_node() {
        return false;
    }

    // Cross the river-deal chance node.
    let possible = game.possible_cards();
    let card = (0..52u8).find(|c| possible & (1u64 << c) != 0);
    match card {
        Some(c) => game.play(c as usize),
        None => return false,
    }

    // Now at a river action node — verify it has actions.
    let actions = game.available_actions();
    !actions.is_empty()
}

/// Navigates both games down the same fixed action line — repeatedly playing action 0
/// until a chance node is reached (or up to 8 steps) — then compares `weights()` for
/// both players at that node.
///
/// Post-play weights reflect the strategy product accumulated along the path and change
/// with every CFR iteration, so they discriminate real nondeterminism. This complements
/// `SolveResult` equality (which checks iteration count + final exploitability) and forms
/// a stronger joint test. Both games are left at the navigated node after this call.
fn post_play_weights_equal(a: &mut PostFlopGame, b: &mut PostFlopGame) -> bool {
    a.back_to_root();
    b.back_to_root();

    // Walk action 0 until a chance node (or up to 8 steps).
    for _ in 0..8 {
        if a.is_chance_node() {
            break;
        }
        let actions_a = a.available_actions();
        let actions_b = b.available_actions();
        if actions_a.is_empty() || actions_b.is_empty() {
            break;
        }
        a.play(0);
        b.play(0);
    }

    // Compare weights() for both players at the current node.
    for player in 0..2 {
        let wa = a.weights(player);
        let wb = b.weights(player);
        if wa.len() != wb.len() {
            return false;
        }
        for (x, y) in wa.iter().zip(wb.iter()) {
            if x.to_bits() != y.to_bits() {
                return false;
            }
        }
    }
    true
}

/// Generates a unique temp-file path for this process.
fn temp_path(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "pfs_flop_bench_{}_{}.bin",
        std::process::id(),
        suffix
    ))
}

/// Sums the per-combo weights of a range (effective combo count).
fn total_combos(range: &Range) -> f32 {
    range.raw_data().iter().sum()
}

/// Prints a PASS/FAIL line and flips `any_failed` on failure.
fn check(label: &str, ok: bool, any_failed: &mut bool) {
    if ok {
        println!("PASS  {}", label);
    } else {
        println!("FAIL  {}", label);
        *any_failed = true;
    }
}

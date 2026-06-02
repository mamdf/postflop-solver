use postflop_solver::*;
use std::time::Instant;

// ============================================================================
// Phase 4 measurement: cost of an on-demand RIVER re-solve (Technique B gate).
//
// Technique B wants to save a SHALLOW artifact (e.g. turn-level) and, when the
// user navigates to a river that was dropped from the save, RE-SOLVE that river
// subgame on demand instead of storing it. The whole bet of Technique B is that
// a single river re-solve is "cheap" (~sub-second). That premise was never
// measured -- the only existing bench (benches/web_case.rs) times a FLOP solve.
// This example measures the real number on a realistic tree.
//
// What this does:
//   1. Solve the flop with a COARSE tree -- only to cheaply generate realistic
//      input ranges that reach a concrete river. The flop fineness barely moves
//      the river COST (which is set by the river action tree + hand count), so a
//      coarse flop keeps range-generation fast.
//   2. Walk flop -> turn -> river, dealing concrete cards.
//   3. Rebuild a river-only game (initial_state = River) with a PRODUCTION-FAT
//      river action tree (33/75/125/allin + 2.5x raise). That fat tree over the
//      ~1300x1300 hand matrix is the conservative (worst-ish) cost we care about.
//   4. Time solve_with_result on the river at 0.5% / 0.3% / 0.1% of pot.
//   5. Check determinism (two identical re-solves must match exactly).
//   6. Secondary: time a TURN re-solve from flop ranges, for the cost-curve
//      contrast (turn-from-flop is the expensive case Technique B must AVOID).
//
// CRITICAL CAVEATS (see docs/technique-b-feasibility.md):
//   - A river-rooted game runs SINGLE-THREAD: enable_parallelization() is false
//     once river != NOT_DEALT (src/game/node.rs). So this measures the true
//     single-thread cost; rayon does not help the river re-solve.
//   - Build with `--release` or every number here is meaningless.
//   - Isomorphism un-swap (weights() is raw) is a Phase-1 CORRECTNESS concern.
//     It does NOT affect re-solve COST, which is all this example measures, so we
//     use concrete cards exactly like examples/turn_resolve.rs.
// ============================================================================

fn main() {
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let flop = flop_from_str("Td9d6h").unwrap();

    let flop_card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop,
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let coarse = BetSizeOptions::try_from(("50%", "")).unwrap();
    let flop_tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 60,
        effective_stack: 400,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [coarse.clone(), coarse.clone()],
        turn_bet_sizes: [coarse.clone(), coarse.clone()],
        river_bet_sizes: [coarse.clone(), coarse.clone()],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let flop_tree = ActionTree::new(flop_tree_config).unwrap();
    let mut flop_game = PostFlopGame::with_config(flop_card_config, flop_tree).unwrap();
    flop_game.allocate_memory(false);

    // ---- 1. Baseline: solve the flop (parallel) ----------------------------
    let t0 = Instant::now();
    let flop_res = solve_with_result(&mut flop_game, 1000, 60.0 * 0.005, false);
    let flop_secs = t0.elapsed().as_secs_f64();
    println!(
        "[baseline] flop solve (parallel): {:.3}s | {} iters | expl {:.4}",
        flop_secs, flop_res.num_iterations, flop_res.exploitability
    );

    // ---- 2. Walk flop -> turn ----------------------------------------------
    play_action(&mut flop_game, |a| matches!(a, Action::Check)); // OOP check
    play_action(&mut flop_game, |a| matches!(a, Action::Bet(_))); // IP bet
    play_action(&mut flop_game, |a| matches!(a, Action::Call)); // OOP call
    assert!(flop_game.is_chance_node());
    let turn_card = card_from_str("Qc").unwrap();
    assert!(flop_game.possible_cards() & (1 << turn_card) != 0);
    flop_game.play(turn_card as usize);

    // Capture the turn ranges/pot/stack BEFORE walking the turn line (for the
    // secondary turn-from-flop measurement at the end).
    let oop_turn_range = extract_range(&flop_game, 0);
    let ip_turn_range = extract_range(&flop_game, 1);
    let [oop_inv_t, ip_inv_t] = flop_game.total_bet_amount();
    let turn_pot = flop_game.tree_config().starting_pot + oop_inv_t + ip_inv_t;
    let turn_stack = flop_game.tree_config().effective_stack - oop_inv_t.max(ip_inv_t);

    // ---- 3. Walk turn -> river ---------------------------------------------
    play_action(&mut flop_game, |a| matches!(a, Action::Check)); // OOP check
    play_action(&mut flop_game, |a| matches!(a, Action::Bet(_))); // IP bet
    play_action(&mut flop_game, |a| matches!(a, Action::Call)); // OOP call
    assert!(flop_game.is_chance_node());
    let river_mask = flop_game.possible_cards();
    let river_card = (0..52u8)
        .find(|c| river_mask & (1 << c) != 0)
        .expect("a possible river card");
    flop_game.play(river_card as usize);

    let [oop_inv_r, ip_inv_r] = flop_game.total_bet_amount();
    let river_pot = flop_game.tree_config().starting_pot + oop_inv_r + ip_inv_r;
    let river_stack = flop_game.tree_config().effective_stack - oop_inv_r.max(ip_inv_r);
    println!(
        "river reached -> turn card #{turn_card}, river card #{river_card} | \
         OOP combos {:.1}, IP combos {:.1} | pot {river_pot}, stack {river_stack}, SPR {:.2}",
        total_combos(&extract_range(&flop_game, 0)),
        total_combos(&extract_range(&flop_game, 1)),
        river_stack as f64 / river_pot as f64,
    );

    // ---- 4. River re-solve, production-fat tree ----------------------------
    let fat = BetSizeOptions::try_from(("33%, 75%, 125%, a", "2.5x")).unwrap();

    // Re-extracts the river ranges from `flop_game` (read-only) and builds a
    // fresh river-only game each call -- a game is consumed once solved, so every
    // measurement needs its own.
    let build_river = || -> PostFlopGame {
        let cc = CardConfig {
            range: [extract_range(&flop_game, 0), extract_range(&flop_game, 1)],
            flop,
            turn: turn_card,
            river: river_card,
        };
        let tc = TreeConfig {
            initial_state: BoardState::River,
            starting_pot: river_pot,
            effective_stack: river_stack,
            rake_rate: 0.0,
            rake_cap: 0.0,
            bubble_factor: [1.0, 1.0],
            flop_bet_sizes: [coarse.clone(), coarse.clone()], // unused on a river game
            turn_bet_sizes: [coarse.clone(), coarse.clone()], // unused on a river game
            river_bet_sizes: [fat.clone(), fat.clone()],
            turn_donk_sizes: None,
            river_donk_sizes: None,
            add_allin_threshold: 1.5,
            force_allin_threshold: 0.15,
            merging_threshold: 0.1,
        };
        let tree = ActionTree::new(tc).unwrap();
        let mut g = PostFlopGame::with_config(cc, tree).unwrap();
        g.allocate_memory(false);
        g
    };

    {
        let g = build_river();
        let (mem, _) = g.memory_usage();
        println!(
            "river game -> {} private hands/player | root actions {:?} | mem {:.1} MB",
            g.num_private_hands(0),
            g.available_actions(),
            mem as f64 / (1024.0 * 1024.0),
        );
    }

    for pct in [0.005f32, 0.003, 0.001] {
        let mut g = build_river();
        let target = river_pot as f32 * pct;
        let t = Instant::now();
        let res = solve_with_result(&mut g, 1000, target, false);
        let secs = t.elapsed().as_secs_f64();
        println!(
            "[river re-solve] target {:>4.1}% pot ({:.3}) -> {:.4}s | {} iters | expl {:.4}",
            pct * 100.0,
            target,
            secs,
            res.num_iterations,
            res.exploitability,
        );
    }

    // ---- 5. Determinism: two identical re-solves must match exactly ---------
    let (mut a, mut b) = (build_river(), build_river());
    let dtarget = river_pot as f32 * 0.001;
    let ra = solve_with_result(&mut a, 1000, dtarget, false);
    let rb = solve_with_result(&mut b, 1000, dtarget, false);
    println!(
        "[determinism] A: {} iters / expl {:.6} | B: {} iters / expl {:.6} -> {}",
        ra.num_iterations,
        ra.exploitability,
        rb.num_iterations,
        rb.exploitability,
        if ra == rb { "IDENTICAL" } else { "DIVERGED" },
    );

    // ---- 5b. WORST CASE: wide ranges, deep SPR, fat tree -------------------
    // The line above is a FAVORABLE river (ranges collapsed to ~100 hands after
    // two bet/call streets). The feasibility report's risk #1 is a WIDE-range
    // river (~1300 hands) at deep SPR. Stress it directly: seed the FULL starting
    // ranges on a concrete 5-card board with a deep stack (SPR ~6.7), so the hand
    // matrix is near-maximal and the fat river tree fans into many bet/raise lines.
    let wc_flop = flop_from_str("Td9d6h").unwrap();
    let wc_turn = card_from_str("Qc").unwrap();
    let wc_river = card_from_str("2s").unwrap();
    let build_wc = || -> PostFlopGame {
        let cc = CardConfig {
            range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
            flop: wc_flop,
            turn: wc_turn,
            river: wc_river,
        };
        let tc = TreeConfig {
            initial_state: BoardState::River,
            starting_pot: 60,
            effective_stack: 400, // SPR ~6.7 -> deep, fat river tree
            rake_rate: 0.0,
            rake_cap: 0.0,
            bubble_factor: [1.0, 1.0],
            flop_bet_sizes: [coarse.clone(), coarse.clone()],
            turn_bet_sizes: [coarse.clone(), coarse.clone()],
            river_bet_sizes: [fat.clone(), fat.clone()],
            turn_donk_sizes: None,
            river_donk_sizes: None,
            add_allin_threshold: 1.5,
            force_allin_threshold: 0.15,
            merging_threshold: 0.1,
        };
        let tree = ActionTree::new(tc).unwrap();
        let mut g = PostFlopGame::with_config(cc, tree).unwrap();
        g.allocate_memory(false);
        g
    };
    {
        let g = build_wc();
        let (mem, _) = g.memory_usage();
        println!(
            "[worst-case] river game -> {} private hands/player | root actions {:?} | mem {:.1} MB",
            g.num_private_hands(0),
            g.available_actions(),
            mem as f64 / (1024.0 * 1024.0),
        );
    }
    for pct in [0.005f32, 0.001] {
        let mut g = build_wc();
        let target = 60.0 * pct;
        let t = Instant::now();
        let res = solve_with_result(&mut g, 1000, target, false);
        println!(
            "[worst-case river] target {:>4.1}% pot -> {:.4}s | {} iters | expl {:.4}",
            pct * 100.0,
            t.elapsed().as_secs_f64(),
            res.num_iterations,
            res.exploitability,
        );
    }

    // ---- 5c. THEORETICAL MAX: 100% vs 100% ranges (~1300 hands) ------------
    // The report's ~1300-hand figure is a FULL (100%) range. Real solve ranges are
    // narrower (167 hands above), but river solve cost scales ~quadratically with
    // hand count at showdown, so the 100%-vs-100% river is the only case that could
    // plausibly approach the ~1s budget. Measure the true ceiling.
    let build_max = || -> PostFlopGame {
        let cc = CardConfig {
            range: [Range::ones(); 2], // 100% vs 100%
            flop: wc_flop,
            turn: wc_turn,
            river: wc_river,
        };
        let tc = TreeConfig {
            initial_state: BoardState::River,
            starting_pot: 60,
            effective_stack: 400, // SPR ~6.7 -> deep, fat river tree
            rake_rate: 0.0,
            rake_cap: 0.0,
            bubble_factor: [1.0, 1.0],
            flop_bet_sizes: [coarse.clone(), coarse.clone()],
            turn_bet_sizes: [coarse.clone(), coarse.clone()],
            river_bet_sizes: [fat.clone(), fat.clone()],
            turn_donk_sizes: None,
            river_donk_sizes: None,
            add_allin_threshold: 1.5,
            force_allin_threshold: 0.15,
            merging_threshold: 0.1,
        };
        let tree = ActionTree::new(tc).unwrap();
        let mut g = PostFlopGame::with_config(cc, tree).unwrap();
        g.allocate_memory(false);
        g
    };
    {
        let g = build_max();
        let (mem, _) = g.memory_usage();
        println!(
            "[max 100%] river game -> {} private hands/player | root actions {:?} | mem {:.1} MB",
            g.num_private_hands(0),
            g.available_actions(),
            mem as f64 / (1024.0 * 1024.0),
        );
    }
    for pct in [0.005f32, 0.001] {
        let mut g = build_max();
        let target = 60.0 * pct;
        let t = Instant::now();
        let res = solve_with_result(&mut g, 1000, target, false);
        println!(
            "[max 100% river] target {:>4.1}% pot -> {:.4}s | {} iters | expl {:.4}",
            pct * 100.0,
            t.elapsed().as_secs_f64(),
            res.num_iterations,
            res.exploitability,
        );
    }

    // ---- 6. Secondary: TURN re-solve from flop ranges (the expensive case) --
    // This is what Technique B must AVOID (a flop-only save forces this). Measured
    // here only to quantify the contrast vs the cheap river re-solve. A turn game
    // runs PARALLEL (river == NOT_DEALT at the turn root), unlike the river.
    // NOTE: deliberately MODERATE sizes (not the river's fat tree) to stay light on
    // WSL2 RAM -- a production-fat turn tree would be heavier still, so the
    // turn >> river contrast measured here is conservative.
    let turn_bets = BetSizeOptions::try_from(("33%, 75%", "")).unwrap();
    let turn_river_bets = BetSizeOptions::try_from(("50%", "")).unwrap();
    let turn_tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: turn_pot,
        effective_stack: turn_stack,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [coarse.clone(), coarse.clone()],
        turn_bet_sizes: [turn_bets.clone(), turn_bets.clone()],
        river_bet_sizes: [turn_river_bets.clone(), turn_river_bets.clone()],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };
    let turn_card_config = CardConfig {
        range: [oop_turn_range, ip_turn_range],
        flop,
        turn: turn_card,
        river: NOT_DEALT,
    };
    let turn_tree = ActionTree::new(turn_tree_config).unwrap();
    let mut turn_game = PostFlopGame::with_config(turn_card_config, turn_tree).unwrap();
    turn_game.allocate_memory(false);
    let (turn_mem, _) = turn_game.memory_usage();
    let t = Instant::now();
    let turn_res = solve_with_result(&mut turn_game, 1000, turn_pot as f32 * 0.005, false);
    let turn_secs = t.elapsed().as_secs_f64();
    println!(
        "[turn re-solve] target 0.5% pot -> {:.4}s | {} iters | expl {:.4} | mem {:.1} MB (parallel)",
        turn_secs,
        turn_res.num_iterations,
        turn_res.exploitability,
        turn_mem as f64 / (1024.0 * 1024.0),
    );
}

/// Plays the first available action at the current node that matches `pred`.
fn play_action(game: &mut PostFlopGame, pred: impl Fn(&Action) -> bool) {
    let actions = game.available_actions();
    let index = actions
        .iter()
        .position(|a| pred(a))
        .unwrap_or_else(|| panic!("no matching action among {actions:?}"));
    game.play(index);
}

/// Builds a `Range` from the reach weights of the given player at the current node.
fn extract_range(game: &PostFlopGame, player: usize) -> Range {
    Range::from_hands_weights(game.private_cards(player), game.weights(player))
        .expect("reach weights are valid range weights")
}

/// Sums the per-combo weights of a range (its effective combo count).
fn total_combos(range: &Range) -> f32 {
    range.raw_data().iter().sum()
}

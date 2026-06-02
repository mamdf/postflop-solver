//! Compact, fully navigable solution save.
//!
//! Run with the `zstd` feature to exercise the compressed path:
//!
//! ```text
//! cargo run --release --example compact_solution --features zstd
//! ```
//!
//! The example solves a small flop spot, saves it with [`save_solution`], reloads it
//! with [`load_solution`], and then navigates into a river node — proving the whole
//! tree is queryable even though a River-mode save stores only the strategy on disk and
//! recomputes the counterfactual values on load.

use postflop_solver::*;

fn main() {
    // A small flop spot: narrow ranges and a low SPR keep the tree, the solve, and the
    // saved file tiny so the example runs in well under a second.
    let oop_range = "AA-TT,AKs,AQs,AJs,KQs";
    let ip_range = "JJ-88,AQs,AJs,KQs,QJs,JTs";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let bet_sizes = BetSizeOptions::try_from(("50%", "60%")).unwrap();
    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 20,
        effective_stack: 40,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        turn_bet_sizes: [bet_sizes.clone(), bet_sizes.clone()],
        river_bet_sizes: [bet_sizes.clone(), bet_sizes],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(false);

    let target_exploitability = game.tree_config().starting_pot as f32 * 0.005;
    let exploitability = solve(&mut game, 200, target_exploitability, false);
    println!("Solved. Exploitability: {exploitability:.4}");

    // Save a compact, fully navigable solution. With the `zstd` feature we also compress
    // it; otherwise we fall back to the always-available uncompressed path.
    let options = if cfg!(feature = "zstd") {
        SaveOptions::compact(12).with_memo("compact navigable demo")
    } else {
        SaveOptions::navigable().with_memo("navigable demo")
    };

    let path = std::env::temp_dir().join("compact_solution_demo.bin");
    save_solution(&mut game, &path, &options).unwrap();
    let file_size = std::fs::metadata(&path).unwrap().len();
    println!(
        "Saved {} ({:.1} KB on disk)",
        path.display(),
        file_size as f64 / 1024.0
    );

    // Load it back. For a River-mode file this recomputes the counterfactual values.
    let (mut loaded, memo) = load_solution(&path, &LoadOptions::default()).unwrap();
    std::fs::remove_file(&path).unwrap();
    println!("Loaded. Memo: {memo:?}");

    let (uncompressed, compressed) = loaded.memory_usage();
    println!(
        "In-memory size: {:.2} MB (f32) / {:.2} MB (u16)",
        uncompressed as f64 / (1024.0 * 1024.0),
        compressed as f64 / (1024.0 * 1024.0),
    );

    // The whole tree is navigable, exactly as if just solved.
    loaded.cache_normalized_weights();
    let avg_equity = compute_average(&loaded.equity(0), loaded.normalized_weights(0));
    println!("Root OOP equity: {:.2}%", 100.0 * avg_equity);

    // Walk into a river decision and read its strategy — data a River-mode save does not
    // store on disk but reconstructs on load.
    navigate_to_a_river_node(&mut loaded);
    if !loaded.is_terminal_node() && !loaded.is_chance_node() {
        let actions = loaded.available_actions();
        let strategy = loaded.strategy();
        println!(
            "Reached a river decision for player {} with {} actions (strategy vector len {})",
            loaded.current_player(),
            actions.len(),
            strategy.len(),
        );
    }
}

/// Plays actions until the current node is a chance node (or terminal). Prefers the
/// passive line (Check, else Call) so a betting round closes quickly.
fn advance_to_next_chance(game: &mut PostFlopGame) {
    while !game.is_chance_node() && !game.is_terminal_node() {
        let actions = game.available_actions();
        let index = actions
            .iter()
            .position(|action| matches!(action, Action::Check))
            .or_else(|| {
                actions
                    .iter()
                    .position(|action| matches!(action, Action::Call))
            })
            .unwrap_or(0);
        game.play(index);
    }
}

/// Navigates from the root to a river decision node: close the flop, deal the turn,
/// close the turn, deal the river.
fn navigate_to_a_river_node(game: &mut PostFlopGame) {
    game.back_to_root();
    advance_to_next_chance(game);
    if game.is_chance_node() {
        game.play(usize::MAX); // deal the first available turn card
    }
    advance_to_next_chance(game);
    if game.is_chance_node() {
        game.play(usize::MAX); // deal the first available river card
    }
}

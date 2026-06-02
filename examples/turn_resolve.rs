use postflop_solver::*;

// ============================================================================
// Turn re-solve from flop ranges.
//
// Idea: solve the flop with a COARSE (cheap) action tree, walk a betting line
// to a turn node, read the ranges that survive that line, and feed them as the
// input ranges of a fresh, FINER-grained turn-only solve. This buys extra
// action detail on the turn at a fraction of the cost of a full detailed solve.
//
// Caveat (read this): this is "subgame resolving". The resulting turn strategy
// is a Nash equilibrium of the *isolated* turn subgame given those fixed input
// ranges -- NOT of the full game. It drops the constraint that the flop strategy
// stays a best response to the (now different) turn play. It is a standard,
// accepted approximation for adding detail cheaply, but it is not equivalent to
// a full, detailed flop-to-river solve. If you need that fidelity instead, keep
// the full game and use node locking (see `examples/node_locking.rs`).
// ============================================================================

fn main() {
    // ---- 1. Solve the flop with a coarse tree ------------------------------
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let flop = flop_from_str("Td9d6h").unwrap();

    let flop_card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop,
        turn: NOT_DEALT, // the full game starts on the flop
        river: NOT_DEALT,
    };

    // coarse: a single 50%-of-pot bet, no raises -> a small, fast tree
    let coarse = BetSizeOptions::try_from(("50%", "")).unwrap();

    let flop_tree_config = TreeConfig {
        initial_state: BoardState::Flop, // must match `card_config`
        starting_pot: 60,
        effective_stack: 400, // deep enough that the finer turn sizings survive
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

    let (flop_mem, _) = flop_game.memory_usage();
    println!(
        "Flop game memory: {:.3} GB",
        flop_mem as f64 / (1024.0 * 1024.0 * 1024.0)
    );

    let flop_target = flop_game.tree_config().starting_pot as f32 * 0.005; // 0.5% of pot
    let flop_expl = solve(&mut flop_game, 1000, flop_target, false);
    println!("Flop solved. Exploitability: {:.4}", flop_expl);

    // ---- 2. Walk a betting line to a turn node -----------------------------
    // Line: OOP check -> IP bet -> OOP call -> deal the turn card.
    play_action(&mut flop_game, |a| matches!(a, Action::Check)); // OOP checks
    play_action(&mut flop_game, |a| matches!(a, Action::Bet(_))); // IP bets
    play_action(&mut flop_game, |a| matches!(a, Action::Call)); // OOP calls

    // we are now at a chance node (the turn): deal "Qc"
    assert!(flop_game.is_chance_node());
    let turn_card = card_from_str("Qc").unwrap();
    assert!(flop_game.possible_cards() & (1 << turn_card) != 0);
    flop_game.play(turn_card as usize);

    // ---- 3. Extract the ranges that reached this turn node ------------------
    // `weights(player)` is the per-hand reach probability: the running product
    // of the strategy probabilities along the line we just played. That product
    // *is* the range arriving at this node. Use `weights()`, NOT
    // `normalized_weights()` -- the latter folds in the opponent's card-removal
    // combinatorics for display/equity averaging, not for input ranges.
    let oop_turn_range = extract_range(&flop_game, 0);
    let ip_turn_range = extract_range(&flop_game, 1);

    println!(
        "Combos reaching the turn -> OOP: {:.1}, IP: {:.1}",
        total_combos(&oop_turn_range),
        total_combos(&ip_turn_range),
    );

    // The turn pot/stack are NOT the flop's: they depend on the line played.
    // `node_pot_stack()` accumulates total investment via `total_bet_amount()` and
    // derives both values from the root `TreeConfig` in one call.
    let (turn_pot, turn_stack) = flop_game.node_pot_stack();
    println!("Turn subgame -> pot: {turn_pot}, effective stack: {turn_stack}");

    // ---- 4. Fresh, finer-grained turn solve --------------------------------
    let fine = BetSizeOptions::try_from(("33%, 75%, 125%, a", "2.5x")).unwrap();

    let turn_card_config = CardConfig {
        range: [oop_turn_range, ip_turn_range],
        flop, // the flop is always required, even when solving from the turn
        turn: turn_card,
        river: NOT_DEALT,
    };

    let turn_tree_config = TreeConfig {
        initial_state: BoardState::Turn, // must match `card_config`
        starting_pot: turn_pot,
        effective_stack: turn_stack,
        rake_rate: 0.0,
        rake_cap: 0.0,
        bubble_factor: [1.0, 1.0],
        flop_bet_sizes: [coarse.clone(), coarse.clone()], // unused on a turn game
        turn_bet_sizes: [fine.clone(), fine.clone()],
        river_bet_sizes: [fine.clone(), fine.clone()],
        turn_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        river_donk_sizes: Some(DonkSizeOptions::try_from("50%").unwrap()),
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.15,
        merging_threshold: 0.1,
    };

    let turn_tree = ActionTree::new(turn_tree_config).unwrap();
    let mut turn_game = PostFlopGame::with_config(turn_card_config, turn_tree).unwrap();
    turn_game.allocate_memory(false);

    let turn_target = turn_game.tree_config().starting_pot as f32 * 0.005;
    let turn_expl = solve(&mut turn_game, 1000, turn_target, false);
    println!("Turn re-solved. Exploitability: {:.4}", turn_expl);

    // The finer tree exposes more sizings at the turn root than the coarse flop
    // solve ever did (a single 50% bet vs. 33% / 75% / 125% / all-in here).
    println!("Turn root actions (OOP): {:?}", turn_game.available_actions());

    // sanity: every hand carried over still resolves to a normalized strategy
    turn_game.cache_normalized_weights();
    let oop_equity = turn_game.equity(0);
    let avg_equity = compute_average(&oop_equity, turn_game.normalized_weights(0));
    println!("OOP average equity on the turn: {:.2}%", 100.0 * avg_equity);
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
    // `private_cards` and `weights` are returned in the same order, so they zip
    // directly into the (hand, weight) pairs `from_hands_weights` expects.
    Range::from_hands_weights(game.private_cards(player), game.weights(player))
        .expect("reach weights are valid range weights")
}

/// Sums the per-combo weights of a range (its effective combo count).
fn total_combos(range: &Range) -> f32 {
    range.raw_data().iter().sum()
}

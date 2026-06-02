use super::*;
use crate::interface::{Game, GameNode};
use crate::range::*;
use crate::solver::*;
use crate::utility::*;
use crate::BunchingData;

#[test]
fn all_check_all_range() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    assert_eq!(game.expected_value_unit(), ExpectedValueUnit::Chips);
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let legacy_ev_oop = game.expected_values(0);
    let legacy_ev_ip = game.expected_values(1);
    assert_eq!(game.reported_expected_values(0), legacy_ev_oop);
    assert_eq!(game.reported_expected_values(1), legacy_ev_ip);
    let ev_oop = compute_average(&legacy_ev_oop, weights_oop);
    let ev_ip = compute_average(&legacy_ev_ip, weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);

    game.play(0);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);

    game.play(0);
    assert!(game.is_chance_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);

    game.play(usize::MAX);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);

    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_terminal_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);
}

#[test]
fn one_raise_all_range() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        river_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 37.5).abs() < 1e-4);
    assert!((ev_ip - 22.5).abs() < 1e-4);

    game.play(0);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 37.5).abs() < 1e-4);
    assert!((ev_ip - 22.5).abs() < 1e-4);

    game.play(0);
    assert!(game.is_chance_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 37.5).abs() < 1e-4);
    assert!((ev_ip - 22.5).abs() < 1e-4);

    game.play(usize::MAX);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 37.5).abs() < 1e-4);
    assert!((ev_ip - 22.5).abs() < 1e-4);

    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(1);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 75.0).abs() < 1e-4);
    assert!((ev_ip - 15.0).abs() < 1e-4);

    game.play(1);
    assert!(game.is_terminal_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 60.0).abs() < 1e-4);
    assert!((ev_ip - 60.0).abs() < 1e-4);
}

#[test]
fn one_raise_all_range_compressed() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        river_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(true);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-4);
    assert!((equity_ip - 0.5).abs() < 1e-4);
    assert!((ev_oop - 37.5).abs() < 1e-2);
    assert!((ev_ip - 22.5).abs() < 1e-2);

    game.play(0);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-4);
    assert!((equity_ip - 0.5).abs() < 1e-4);
    assert!((ev_oop - 37.5).abs() < 1e-2);
    assert!((ev_ip - 22.5).abs() < 1e-2);

    game.play(0);
    assert!(game.is_chance_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-4);
    assert!((equity_ip - 0.5).abs() < 1e-4);
    assert!((ev_oop - 37.5).abs() < 1e-2);
    assert!((ev_ip - 22.5).abs() < 1e-2);

    game.play(usize::MAX);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-4);
    assert!((equity_ip - 0.5).abs() < 1e-4);
    assert!((ev_oop - 37.5).abs() < 1e-2);
    assert!((ev_ip - 22.5).abs() < 1e-2);

    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(1);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-4);
    assert!((equity_ip - 0.5).abs() < 1e-4);
    assert!((ev_oop - 75.0).abs() < 1e-2);
    assert!((ev_ip - 15.0).abs() < 1e-2);

    game.play(1);
    assert!(game.is_terminal_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-4);
    assert!((equity_ip - 0.5).abs() < 1e-4);
    assert!((ev_oop - 60.0).abs() < 1e-2);
    assert!((ev_ip - 60.0).abs() < 1e-2);
}

#[test]
fn one_raise_all_range_with_turn() {
    let card_config = CardConfig {
        flop: flop_from_str("Td9d6h").unwrap(),
        range: [Range::ones(); 2],
        turn: card_from_str("Qc").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: 60,
        effective_stack: 970,
        river_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_equity_oop = compute_average(&game.equity(0), weights_oop);
    let root_equity_ip = compute_average(&game.equity(1), weights_ip);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    assert!((root_equity_oop - 0.5).abs() < 1e-5);
    assert!((root_equity_ip - 0.5).abs() < 1e-5);
    assert!((root_ev_oop - 37.5).abs() < 1e-4);
    assert!((root_ev_ip - 22.5).abs() < 1e-4);
}

#[test]
fn one_raise_all_range_with_river() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: card_from_str("7s").unwrap(),
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 60,
        effective_stack: 970,
        river_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 37.5).abs() < 1e-4);
    assert!((ev_ip - 22.5).abs() < 1e-4);

    game.play(0);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);

    game.play(0);
    assert!(game.is_terminal_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 30.0).abs() < 1e-4);
    assert!((ev_ip - 30.0).abs() < 1e-4);

    game.back_to_root();
    game.play(1);
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 75.0).abs() < 1e-4);
    assert!((ev_ip - 15.0).abs() < 1e-4);

    game.play(0);
    assert!(game.is_terminal_node());
    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!(game.is_terminal_node());
    assert!((equity_oop - 0.5).abs() < 1e-5);
    assert!((equity_ip - 0.5).abs() < 1e-5);
    assert!((ev_oop - 90.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);
}

#[test]
fn always_win() {
    // be careful for straight flushes
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), lose_range_str.parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 1.0).abs() < 1e-5);
    assert!((equity_ip - 0.0).abs() < 1e-5);
    assert!((ev_oop - 60.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);

    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_terminal_node());

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 1.0).abs() < 1e-5);
    assert!((equity_ip - 0.0).abs() < 1e-5);
    assert!((ev_oop - 60.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);
}

#[test]
fn bubble_factor_changes_terminal_payoff_vs_chipev() {
    let chip_ev = always_win_average_ev([1.0, 1.0]);
    let icm_ev = always_win_average_ev([2.0, 1.0]);

    // ChipEV baseline: OOP always wins the 60-chip pot, IP always loses.
    assert!((chip_ev[0] - 60.0).abs() < 1e-4);
    assert!((chip_ev[1] - 0.0).abs() < 1e-4);

    // With OOP bubble factor 2.0, only OOP's winning payoff is discounted.
    // The displayed EV keeps the existing half-pot bias, so the expected value
    // becomes half_pot + half_pot / BF = 30 + 30 / 2 = 45.
    assert!((icm_ev[0] - 45.0).abs() < 1e-4);
    assert!((icm_ev[1] - 0.0).abs() < 1e-4);
    assert!(icm_ev[0] < chip_ev[0]);
}

#[test]
fn solve_with_result_reports_executed_iterations() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        river_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(false);

    let result = solve_with_result(&mut game, 3, 0.0, false);

    assert_eq!(result.num_iterations, 3);
    assert!(result.exploitability.is_finite());
}

#[test]
fn tournament_icm_changes_internal_ev() {
    // be careful for straight flushes
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), lose_range_str.parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    let utility_count = game
        .set_tournament_icm_config(TournamentIcmConfig {
            stacks: vec![1000.0, 1000.0],
            payouts: vec![70, 30],
            oop_seat: 0,
            ip_seat: 1,
        })
        .unwrap();

    assert!(game.uses_tournament_icm());
    assert_eq!(game.tournament_icm_utility_count(), utility_count);
    assert!(utility_count > 0);

    let expected_oop_delta = (30.0 + 40.0 * 1060.0 / 2060.0 - 50.0) as f32;
    assert!((game.exploitability_target_scale() - expected_oop_delta).abs() < 1e-4);
    assert!(
        (game.target_exploitability_from_fraction(0.005) - expected_oop_delta * 0.005).abs() < 1e-7
    );

    game.allocate_memory(false);
    finalize(&mut game);
    game.cache_normalized_weights();

    assert_eq!(game.expected_value_unit(), ExpectedValueUnit::PayoutDelta);

    let current_ev = compute_current_ev(&game);
    let legacy_ev = game.expected_values(0);
    let reported_ev = game.reported_expected_values(0);

    assert!(current_ev[0] > 0.0);
    assert!(current_ev[1] < 0.0);
    assert!((current_ev[0] - expected_oop_delta).abs() < 1e-4);
    assert!((current_ev[1] + expected_oop_delta).abs() < 1e-4);
    assert!(legacy_ev
        .iter()
        .zip(reported_ev.iter())
        .any(|(&legacy, &reported)| (legacy - reported).abs() > 1.0));
}

#[test]
fn tournament_icm_reported_fold_action_ev_is_zero_and_non_fold_is_delta() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: card_from_str("7s").unwrap(),
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 60,
        effective_stack: 30,
        river_bet_sizes: [("a", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.set_tournament_icm_config(TournamentIcmConfig {
        stacks: vec![1000.0, 1000.0],
        payouts: vec![70, 30],
        oop_seat: 0,
        ip_seat: 1,
    })
    .unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    let allin_index = game
        .available_actions()
        .iter()
        .position(|&action| action == Action::AllIn(30))
        .unwrap();
    game.play(allin_index);
    game.cache_normalized_weights();

    let player = game.current_player();
    let num_hands = game.num_private_hands(player);
    let fold_index = game
        .available_actions()
        .iter()
        .position(|&action| action == Action::Fold)
        .unwrap();
    let legacy_detail = game.expected_values_detail(player);
    let reported_detail = game.reported_expected_values_detail(player);
    let weights = game.normalized_weights(player);

    let fold_legacy = &legacy_detail[fold_index * num_hands..(fold_index + 1) * num_hands];
    let fold_reported = &reported_detail[fold_index * num_hands..(fold_index + 1) * num_hands];
    let non_fold_reported = reported_detail
        .chunks_exact(num_hands)
        .enumerate()
        .filter(|(action_index, _)| *action_index != fold_index)
        .flat_map(|(_, row)| row.iter());

    assert!(fold_legacy
        .iter()
        .zip(weights.iter())
        .all(|(&ev, &weight)| weight == 0.0 || ev == 0.0));
    assert!(fold_reported
        .iter()
        .zip(weights.iter())
        .all(|(&ev, &weight)| weight == 0.0 || ev.abs() < 1e-7));
    assert!(non_fold_reported
        .zip(weights.iter().cycle())
        .any(|(&ev, &weight)| weight > 0.0 && ev.abs() > 1e-7));
}

#[test]
fn tournament_icm_reported_no_fold_node_uses_hypothetical_fold_equivalent() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: card_from_str("7s").unwrap(),
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 60,
        effective_stack: 30,
        river_bet_sizes: [("a", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.set_tournament_icm_config(TournamentIcmConfig {
        stacks: vec![1000.0, 1000.0],
        payouts: vec![70, 30],
        oop_seat: 0,
        ip_seat: 1,
    })
    .unwrap();

    game.allocate_memory(false);
    finalize(&mut game);
    game.cache_normalized_weights();

    assert!(!game.available_actions().contains(&Action::Fold));

    let player = game.current_player();
    let num_hands = game.num_private_hands(player);
    let num_actions = game.available_actions().len();
    let fold_equivalent = game
        .tournament_icm_fold_equivalent_delta(player, game.total_bet_amount())
        .unwrap();
    let raw_detail = game.reported_expected_values_detail_uncentered_for_test(player);
    let reported_detail = game.reported_expected_values_detail(player);
    let legacy_detail = game.expected_values_detail(player);
    let strategy = game.strategy();
    let weights = game.normalized_weights(player);

    assert!(fold_equivalent.abs() > 1e-7_f32);
    assert_eq!(raw_detail.len(), num_actions * num_hands);
    assert_eq!(reported_detail.len(), raw_detail.len());

    for (hand_index, &weight) in weights.iter().enumerate() {
        if weight == 0.0 {
            continue;
        }

        for action_index in 0..num_actions {
            let index = hand_index + action_index * num_hands;
            assert!(
                (reported_detail[index] - (raw_detail[index] - fold_equivalent)).abs() < 1e-6_f32
            );
        }
    }

    assert!(raw_detail
        .iter()
        .zip(reported_detail.iter())
        .zip(weights.iter().cycle())
        .any(|((&raw, &reported), &weight)| weight > 0.0 && (raw - reported).abs() > 1e-7_f32));
    assert!(
        legacy_detail
            .iter()
            .zip(reported_detail.iter())
            .zip(weights.iter().cycle())
            .any(|((&legacy, &reported), &weight)| weight > 0.0
                && (legacy - reported).abs() > 1.0_f32)
    );

    let raw_current_ev: Vec<_> = (0..num_hands)
        .map(|hand_index| {
            (0..num_actions)
                .map(|action_index| {
                    let index = hand_index + action_index * num_hands;
                    raw_detail[index] * strategy[index]
                })
                .sum::<f32>()
        })
        .collect();
    let reported_current_ev = game.reported_expected_values(player);

    assert!(raw_current_ev
        .iter()
        .zip(reported_current_ev.iter())
        .zip(weights.iter())
        .any(|((&raw, &reported), &weight)| {
            weight > 0.0 && (reported - (raw - fold_equivalent)).abs() < 1e-6_f32
        }));
}

#[test]
fn tournament_icm_fold_uses_terminal_contributions() {
    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_from_str("Td9d6h").unwrap(),
        turn: card_from_str("Qc").unwrap(),
        river: card_from_str("7s").unwrap(),
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 60,
        effective_stack: 30,
        river_bet_sizes: [("a", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.set_tournament_icm_config(TournamentIcmConfig {
        stacks: vec![1000.0, 1000.0],
        payouts: vec![70, 30],
        oop_seat: 0,
        ip_seat: 1,
    })
    .unwrap();

    let allin_index = {
        let root = game.root();
        root.children()
            .iter()
            .position(|child| child.lock().prev_action == Action::AllIn(30))
            .unwrap()
    };

    let (fold_amount, utility) = {
        let root = game.root();
        let allin = root.play(allin_index);
        allin
            .children()
            .iter()
            .find_map(|child| {
                let child = child.lock();
                (child.prev_action == Action::Fold).then(|| {
                    (
                        child.amount,
                        game.terminal_icm_utility_for_node(&child).unwrap(),
                    )
                })
            })
            .unwrap()
    };
    let expected_oop_win = 30.0 + 40.0 * 1060.0 / 2060.0 - 50.0;

    assert_eq!(fold_amount, 0);
    assert!((utility.win[0] - expected_oop_win as f32).abs() < 1e-5);
    assert!((utility.lose[1] + expected_oop_win as f32).abs() < 1e-5);
    assert!(utility.win[0] > 0.0);
}

fn always_win_average_ev(bubble_factor: [f64; 2]) -> [f32; 2] {
    // be careful for straight flushes
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), lose_range_str.parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        bubble_factor,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);
    assert_eq!(game.uses_bubble_factor(), bubble_factor != [1.0, 1.0]);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    [
        compute_average(&game.expected_values(0), weights_oop),
        compute_average(&game.expected_values(1), weights_ip),
    ]
}

#[test]
fn always_win_raked() {
    // be careful for straight flushes
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), lose_range_str.parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        rake_rate: 0.05,
        rake_cap: 10.0,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((ev_oop - 57.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);

    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_terminal_node());

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((ev_oop - 57.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);
}

#[test]
fn always_lose() {
    // be careful for straight flushes
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: [lose_range_str.parse().unwrap(), "AA".parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_equity_oop = compute_average(&game.equity(0), weights_oop);
    let root_equity_ip = compute_average(&game.equity(1), weights_ip);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    assert!((root_equity_oop - 0.0).abs() < 1e-5);
    assert!((root_equity_ip - 1.0).abs() < 1e-5);
    assert!((root_ev_oop - 0.0).abs() < 1e-4);
    assert!((root_ev_ip - 60.0).abs() < 1e-4);
}

#[test]
fn always_lose_raked() {
    // be careful for straight flushes
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";
    let card_config = CardConfig {
        range: [lose_range_str.parse().unwrap(), "AA".parse().unwrap()],
        flop: flop_from_str("AcAdKh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        rake_rate: 0.05,
        rake_cap: 10.0,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    assert!((root_ev_oop - 0.0).abs() < 1e-4);
    assert!((root_ev_ip - 57.0).abs() < 1e-4);
}

#[test]
fn always_tie() {
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), "AA".parse().unwrap()],
        flop: flop_from_str("2c6dTh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_equity_oop = compute_average(&game.equity(0), weights_oop);
    let root_equity_ip = compute_average(&game.equity(1), weights_ip);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    assert!((root_equity_oop - 0.5).abs() < 1e-5);
    assert!((root_equity_ip - 0.5).abs() < 1e-5);
    assert!((root_ev_oop - 30.0).abs() < 1e-4);
    assert!((root_ev_ip - 30.0).abs() < 1e-4);
}

#[test]
fn always_tie_raked() {
    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), "AA".parse().unwrap()],
        flop: flop_from_str("2c6dTh").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        rake_rate: 0.05,
        rake_cap: 10.0,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    assert!((root_ev_oop - 28.5).abs() < 1e-4);
    assert!((root_ev_ip - 28.5).abs() < 1e-4);
}

#[test]
fn no_assignment() {
    let card_config = CardConfig {
        range: ["TT".parse().unwrap(), "TT".parse().unwrap()],
        flop: flop_from_str("Td9d6h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let game = PostFlopGame::with_config(card_config, action_tree);
    assert!(game.is_err());
}

#[test]
fn remove_lines() {
    use crate::bet_size::BetSizeOptions;
    let card_config = CardConfig {
        range: ["TT+,AKo,AQs+".parse().unwrap(), "AA".parse().unwrap()],
        flop: flop_from_str("2c6dTh").unwrap(),
        ..Default::default()
    };

    // simple tree: force checks on flop, and only use 1/2 pot bets on turn and river
    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        turn_bet_sizes: [
            BetSizeOptions::try_from(("50%", "")).unwrap(),
            Default::default(),
        ],
        river_bet_sizes: [
            BetSizeOptions::try_from(("50%", "")).unwrap(),
            Default::default(),
        ],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    let lines = vec![
        vec![
            Action::Check,
            Action::Check,
            Action::Chance(2),
            Action::Check,
        ],
        vec![
            Action::Check,
            Action::Check,
            Action::Chance(2),
            Action::Bet(30),
            Action::Call,
            Action::Chance(3),
            Action::Bet(60),
        ],
    ];

    let res = game.remove_lines(&lines);
    assert!(res.is_ok());

    game.allocate_memory(false);

    // check that the turn line is removed
    game.apply_history(&[0, 0, 2]);
    assert_eq!(game.available_actions(), vec![Action::Bet(30)]);

    // check that other turn lines are correct
    game.apply_history(&[0, 0, 3]);
    assert_eq!(
        game.available_actions(),
        vec![Action::Check, Action::Bet(30)]
    );

    // check that the river line is removed
    game.apply_history(&[0, 0, 2, 0, 1, 3]);
    assert_eq!(game.available_actions(), vec![Action::Check]);

    // check that other river lines are correct
    game.apply_history(&[0, 0, 2, 0, 1, 4]);
    assert_eq!(
        game.available_actions(),
        vec![Action::Check, Action::Bet(60)]
    );

    game.apply_history(&[0, 0, 3, 1, 1, 4]);
    assert_eq!(
        game.available_actions(),
        vec![Action::Check, Action::Bet(60)]
    );

    // check that `solve()` does not crash
    solve(&mut game, 10, 0.01, false);
}

#[test]
fn isomorphism_monotone() {
    let oop_range = "88+,A8s+,A5s-A2s:0.5,AJo+,ATo:0.75,K9s+,KQo,KJo:0.75,KTo:0.25,Q9s+,QJo:0.5,J8s+,JTo:0.25,T8s+,T7s:0.45,97s+,96s:0.45,87s,86s:0.75,85s:0.45,75s+:0.75,74s:0.45,65s:0.75,64s:0.5,63s:0.45,54s:0.75,53s:0.5,52s:0.45,43s:0.5,42s:0.45,32s:0.45";
    let ip_range = "AA:0.25,99-22,AJs-A2s,AQo-A8o,K2s+,K9o+,Q2s+,Q9o+,J6s+,J9o+,T6s+,T9o,96s+,95s:0.5,98o,86s+,85s:0.5,75s+,74s:0.5,64s+,63s:0.5,54s,53s:0.5,43s";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("QhJh2h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 100,
        effective_stack: 100,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    let mut check = |history: &[usize],
                     expected_turn_swap: Option<u8>,
                     expected_river_swap: Option<(u8, u8)>| {
        game.apply_history(history);
        game.cache_normalized_weights();
        let weights = game.normalized_weights(0);
        let ev = game.expected_values(0);
        weights.iter().zip(ev.iter()).for_each(|(&w, &v)| {
            assert!(!(w > 0.0 && v == 50.0));
        });
        assert_eq!(game.turn_swap, expected_turn_swap);
        assert_eq!(game.river_swap, expected_river_swap);
    };

    check(&[0, 0, 4], None, None);
    check(&[0, 0, 5], Some(1), None);
    check(&[0, 0, 6], None, None);
    check(&[0, 0, 7], Some(3), None);

    check(&[0, 0, 4, 0, 0, 8], None, None);
    check(&[0, 0, 4, 0, 0, 9], None, None);
    check(&[0, 0, 4, 0, 0, 10], None, None);
    check(&[0, 0, 4, 0, 0, 11], None, Some((0, 3)));

    check(&[0, 0, 5, 0, 0, 8], Some(1), None);
    check(&[0, 0, 5, 0, 0, 9], Some(1), None);
    check(&[0, 0, 5, 0, 0, 10], Some(1), None);
    check(&[0, 0, 5, 0, 0, 11], Some(1), Some((1, 3)));

    check(&[0, 0, 6, 0, 0, 8], None, None);
    check(&[0, 0, 6, 0, 0, 9], None, Some((2, 1)));
    check(&[0, 0, 6, 0, 0, 10], None, None);
    check(&[0, 0, 6, 0, 0, 11], None, Some((2, 3)));

    check(&[0, 0, 7, 0, 0, 8], Some(3), Some((3, 1)));
    check(&[0, 0, 7, 0, 0, 9], Some(3), None);
    check(&[0, 0, 7, 0, 0, 10], Some(3), None);
    check(&[0, 0, 7, 0, 0, 11], Some(3), None);
}

#[test]
fn node_locking() {
    let card_config = CardConfig {
        range: ["AsAh,QsQh".parse().unwrap(), "KsKh".parse().unwrap()],
        flop: flop_from_str("2s3h4d").unwrap(),
        turn: card_from_str("6c").unwrap(),
        river: card_from_str("7c").unwrap(),
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 20,
        effective_stack: 10,
        river_bet_sizes: [("a", "").try_into().unwrap(), ("a", "").try_into().unwrap()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    game.play(1); // all-in
    game.lock_current_strategy(&[0.25, 0.75]); // 25% fold, 75% call
    game.back_to_root();

    solve(&mut game, 1000, 0.0, false);
    game.cache_normalized_weights();

    let ev_oop = game.expected_values(0);
    let ev_ip = game.expected_values(1);
    assert!((ev_oop[0] - 0.0).abs() < 1e-2);
    assert!((ev_oop[1] - 27.5).abs() < 5e-2);
    assert!((ev_ip[0] - 6.25).abs() < 1e-2);

    let strategy_oop = game.strategy();
    assert!((strategy_oop[0] - 1.0).abs() < 1e-3); // QQ check
    assert!((strategy_oop[1] - 0.0).abs() < 1e-3); // AA check
    assert!((strategy_oop[2] - 0.0).abs() < 1e-3); // QQ bet
    assert!((strategy_oop[3] - 1.0).abs() < 1e-3); // AA bet

    game.allocate_memory(false);
    game.play(1); // all-in
    game.lock_current_strategy(&[0.5, 0.5]); // 50% fold, 50% call
    game.back_to_root();

    solve(&mut game, 1000, 0.0, false);
    game.cache_normalized_weights();

    let ev_oop = game.expected_values(0);
    let ev_ip = game.expected_values(1);
    assert!((ev_oop[0] - 5.0).abs() < 1e-2);
    assert!((ev_oop[1] - 25.0).abs() < 5e-2);
    assert!((ev_ip[0] - 5.0).abs() < 1e-2);

    let strategy_oop = game.strategy();
    assert!((strategy_oop[0] - 0.0).abs() < 1e-3); // QQ check
    assert!((strategy_oop[1] - 0.0).abs() < 1e-3); // AA check
    assert!((strategy_oop[2] - 1.0).abs() < 1e-3); // QQ bet
    assert!((strategy_oop[3] - 1.0).abs() < 1e-3); // AA bet
}

#[test]
fn node_locking_partial() {
    let card_config = CardConfig {
        range: ["AsAh,QsQh,JsJh".parse().unwrap(), "KsKh".parse().unwrap()],
        flop: flop_from_str("2s3h4d").unwrap(),
        turn: card_from_str("6c").unwrap(),
        river: card_from_str("7c").unwrap(),
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 10,
        effective_stack: 10,
        river_bet_sizes: [("a", "").try_into().unwrap(), ("a", "").try_into().unwrap()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    game.lock_current_strategy(&[0.8, 0.0, 0.0, 0.2, 0.0, 0.0]); // JJ -> 80% check, 20% all-in

    solve(&mut game, 1000, 0.0, false);
    game.cache_normalized_weights();

    let ev_oop = game.expected_values(0);
    let ev_ip = game.expected_values(1);
    assert!((ev_oop[0] - 0.0).abs() < 1e-2);
    assert!((ev_oop[1] - 0.0).abs() < 1e-2);
    assert!((ev_oop[2] - 15.0).abs() < 5e-2);
    assert!((ev_ip[0] - 5.0).abs() < 1e-2);

    let strategy_oop = game.strategy();
    assert!((strategy_oop[0] - 0.8).abs() < 1e-3); // JJ check
    assert!((strategy_oop[1] - 0.7).abs() < 1e-3); // QQ check
    assert!((strategy_oop[2] - 0.0).abs() < 1e-3); // AA check
    assert!((strategy_oop[3] - 0.2).abs() < 1e-3); // JJ bet
    assert!((strategy_oop[4] - 0.3).abs() < 1e-3); // QQ bet
    assert!((strategy_oop[5] - 1.0).abs() < 1e-3); // AA bet
}

#[test]
fn node_locking_isomorphism() {
    let card_config = CardConfig {
        range: ["AKs".parse().unwrap(), "AKs".parse().unwrap()],
        flop: flop_from_str("2c3c4c").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 10,
        effective_stack: 10,
        river_bet_sizes: [("a", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    game.allocate_memory(false);
    game.apply_history(&[0, 0, 15, 0, 0, 14]); // Turn: Spades, River: Hearts
    game.lock_current_strategy(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0]); // AhKh -> check

    finalize(&mut game);

    game.apply_history(&[0, 0, 13, 0, 0, 14]);
    assert_eq!(
        game.strategy(),
        vec![0.5, 0.5, 1.0, 0.5, 0.5, 0.5, 0.0, 0.5]
    );

    game.apply_history(&[0, 0, 13, 0, 0, 15]);
    assert_eq!(
        game.strategy(),
        vec![0.5, 0.5, 0.5, 1.0, 0.5, 0.5, 0.5, 0.0]
    );

    game.apply_history(&[0, 0, 14, 0, 0, 13]);
    assert_eq!(
        game.strategy(),
        vec![0.5, 1.0, 0.5, 0.5, 0.5, 0.0, 0.5, 0.5]
    );

    game.apply_history(&[0, 0, 14, 0, 0, 15]);
    assert_eq!(
        game.strategy(),
        vec![0.5, 0.5, 0.5, 1.0, 0.5, 0.5, 0.5, 0.0]
    );

    game.apply_history(&[0, 0, 15, 0, 0, 13]);
    assert_eq!(
        game.strategy(),
        vec![0.5, 1.0, 0.5, 0.5, 0.5, 0.0, 0.5, 0.5]
    );

    game.apply_history(&[0, 0, 15, 0, 0, 14]);
    assert_eq!(
        game.strategy(),
        vec![0.5, 0.5, 1.0, 0.5, 0.5, 0.5, 0.0, 0.5]
    );
}

#[test]
fn set_bunching_effect() {
    let flop = flop_from_str("Td9d6h").unwrap();
    let card_config = CardConfig {
        flop,
        range: [Range::ones(); 2],
        turn: card_from_str("Qc").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: 60,
        effective_stack: 970,
        river_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    let co_range = "33:0.59,22:0.635,A8o:0.265,A7o-A6o,A5o:0.445,A4o-A2o,K2s,K9o:0.905,K8o-K2o,Q4s-Q2s,Q9o-Q2o,J6s-J2s,J9o:0.88,J8o-J2o,T7s:0.405,T6s-T2s,T9o:0.96,T8o-T2o,96s-92s,92o+,86s:0.57,85s-82s,82o+,76s:0.37,75s-72s,72o+,65s:0.475,64s-62s,62o+,54s:0.68,53s-52s,52o+,42+,32";
    let sb_range = "66:0.46,55:0.821,44:0.92,33:0.93,22:0.925,A6s:0.73,A3s:0.47,A2s,ATo:0.105,A9o-A2o,K8s:0.795,K7s,K6s:0.85,K5s:0.965,K4s-K2s,KJo:0.085,KTo:0.645,K9o-K2o,Q8s-Q2s,QJo:0.765,QTo-Q2o,J8s-J2s,J2o+,T8s:0.69,T7s-T2s,T2o+,98s:0.905,97s-92s,92o+,87s:0.78,86s-82s,82o+,76s:0.77,75s-72s,72o+,65s:0.845,64s-62s,62o+,54s:0.735,53s-52s,52o+,42+,32";

    let mut bunching_data = BunchingData::new(
        &[co_range.parse().unwrap(), sb_range.parse().unwrap()],
        flop,
    )
    .unwrap();

    bunching_data.process(false);
    game.set_bunching_effect(&bunching_data).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    let current_ev = compute_current_ev(&game);
    assert!((current_ev[0] - 7.5).abs() < 1e-4);
    assert!((current_ev[1] - -7.5).abs() < 1e-4);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_equity_oop = compute_average(&game.equity(0), weights_oop);
    let root_equity_ip = compute_average(&game.equity(1), weights_ip);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    assert!((root_equity_oop - 0.5).abs() < 1e-5);
    assert!((root_equity_ip - 0.5).abs() < 1e-5);
    assert!((root_ev_oop - 37.5).abs() < 1e-4);
    assert!((root_ev_ip - 22.5).abs() < 1e-4);
}

#[test]
fn set_bunching_effect_always_win() {
    let flop = flop_from_str("AcAdKh").unwrap();
    let lose_range_str = "KK-22,K9-K2,Q8-Q2,J8-J2,T8-T2,92+,82+,72+,62+";

    let card_config = CardConfig {
        range: ["AA".parse().unwrap(), lose_range_str.parse().unwrap()],
        flop,
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 970,
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    let co_range = "33:0.59,22:0.635,A8o:0.265,A7o-A6o,A5o:0.445,A4o-A2o,K2s,K9o:0.905,K8o-K2o,Q4s-Q2s,Q9o-Q2o,J6s-J2s,J9o:0.88,J8o-J2o,T7s:0.405,T6s-T2s,T9o:0.96,T8o-T2o,96s-92s,92o+,86s:0.57,85s-82s,82o+,76s:0.37,75s-72s,72o+,65s:0.475,64s-62s,62o+,54s:0.68,53s-52s,52o+,42+,32";
    let sb_range = "66:0.46,55:0.821,44:0.92,33:0.93,22:0.925,A6s:0.73,A3s:0.47,A2s,ATo:0.105,A9o-A2o,K8s:0.795,K7s,K6s:0.85,K5s:0.965,K4s-K2s,KJo:0.085,KTo:0.645,K9o-K2o,Q8s-Q2s,QJo:0.765,QTo-Q2o,J8s-J2s,J2o+,T8s:0.69,T7s-T2s,T2o+,98s:0.905,97s-92s,92o+,87s:0.78,86s-82s,82o+,76s:0.77,75s-72s,72o+,65s:0.845,64s-62s,62o+,54s:0.735,53s-52s,52o+,42+,32";

    let mut bunching_data = BunchingData::new(
        &[co_range.parse().unwrap(), sb_range.parse().unwrap()],
        flop,
    )
    .unwrap();

    bunching_data.process(false);
    game.set_bunching_effect(&bunching_data).unwrap();

    game.allocate_memory(false);
    finalize(&mut game);

    let current_ev = compute_current_ev(&game);
    assert!((current_ev[0] - 30.0).abs() < 1e-4);
    assert!((current_ev[1] - -30.0).abs() < 1e-4);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 1.0).abs() < 1e-5);
    assert!((equity_ip - 0.0).abs() < 1e-5);
    assert!((ev_oop - 60.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);

    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_chance_node());
    game.play(usize::MAX);
    game.play(0);
    game.play(0);
    assert!(game.is_terminal_node());

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let equity_oop = compute_average(&game.equity(0), weights_oop);
    let equity_ip = compute_average(&game.equity(1), weights_ip);
    let ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let ev_ip = compute_average(&game.expected_values(1), weights_ip);
    assert!((equity_oop - 1.0).abs() < 1e-5);
    assert!((equity_ip - 0.0).abs() < 1e-5);
    assert!((ev_oop - 60.0).abs() < 1e-4);
    assert!((ev_ip - 0.0).abs() < 1e-4);
}

#[test]
#[ignore]
fn solve_pio_preset_normal() {
    let oop_range = "88+,A8s+,A5s-A2s:0.5,AJo+,ATo:0.75,K9s+,KQo,KJo:0.75,KTo:0.25,Q9s+,QJo:0.5,J8s+,JTo:0.25,T8s+,T7s:0.45,97s+,96s:0.45,87s,86s:0.75,85s:0.45,75s+:0.75,74s:0.45,65s:0.75,64s:0.5,63s:0.45,54s:0.75,53s:0.5,52s:0.45,43s:0.5,42s:0.45,32s:0.45";
    let ip_range = "AA:0.25,99-22,AJs-A2s,AQo-A8o,K2s+,K9o+,Q2s+,Q9o+,J6s+,J9o+,T6s+,T9o,96s+,95s:0.5,98o,86s+,85s:0.5,75s+,74s:0.5,64s+,63s:0.5,54s,53s:0.5,43s";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("QsJh2h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 180,
        effective_stack: 910,
        flop_bet_sizes: [
            ("52%", "45%").try_into().unwrap(),
            ("52%", "45%").try_into().unwrap(),
        ],
        turn_bet_sizes: [
            ("55%", "45%").try_into().unwrap(),
            ("55%", "45%").try_into().unwrap(),
        ],
        river_bet_sizes: [
            ("70%", "45%").try_into().unwrap(),
            ("70%", "45%").try_into().unwrap(),
        ],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    println!(
        "memory usage: {:.2}GB",
        game.memory_usage().0 as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    game.allocate_memory(false);

    solve(&mut game, 1000, 180.0 * 0.001, true);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_equity_oop = compute_average(&game.equity(0), weights_oop);
    let root_equity_ip = compute_average(&game.equity(1), weights_ip);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    // verified by PioSOLVER Free
    assert!((root_equity_oop - 0.55347).abs() < 1e-5);
    assert!((root_equity_ip - 0.44653).abs() < 1e-5);
    assert!((root_ev_oop - 105.11).abs() < 0.2);
    assert!((root_ev_ip - 74.89).abs() < 0.2);
}

#[test]
#[ignore]
fn solve_pio_preset_raked() {
    let oop_range = "88+,A8s+,A5s-A2s:0.5,AJo+,ATo:0.75,K9s+,KQo,KJo:0.75,KTo:0.25,Q9s+,QJo:0.5,J8s+,JTo:0.25,T8s+,T7s:0.45,97s+,96s:0.45,87s,86s:0.75,85s:0.45,75s+:0.75,74s:0.45,65s:0.75,64s:0.5,63s:0.45,54s:0.75,53s:0.5,52s:0.45,43s:0.5,42s:0.45,32s:0.45";
    let ip_range = "AA:0.25,99-22,AJs-A2s,AQo-A8o,K2s+,K9o+,Q2s+,Q9o+,J6s+,J9o+,T6s+,T9o,96s+,95s:0.5,98o,86s+,85s:0.5,75s+,74s:0.5,64s+,63s:0.5,54s,53s:0.5,43s";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("QsJh2h").unwrap(),
        ..Default::default()
    };

    let tree_config = TreeConfig {
        starting_pot: 180,
        effective_stack: 910,
        rake_rate: 0.05,
        rake_cap: 30.0,
        flop_bet_sizes: [
            ("52%", "45%").try_into().unwrap(),
            ("52%", "45%").try_into().unwrap(),
        ],
        turn_bet_sizes: [
            ("55%", "45%").try_into().unwrap(),
            ("55%", "45%").try_into().unwrap(),
        ],
        river_bet_sizes: [
            ("70%", "45%").try_into().unwrap(),
            ("70%", "45%").try_into().unwrap(),
        ],
        ..Default::default()
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    println!(
        "memory usage: {:.2}GB",
        game.memory_usage().0 as f64 / (1024.0 * 1024.0 * 1024.0)
    );
    game.allocate_memory(false);

    solve(&mut game, 1000, 180.0 * 0.001, true);

    game.cache_normalized_weights();
    let weights_oop = game.normalized_weights(0);
    let weights_ip = game.normalized_weights(1);
    let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
    let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

    // verified by PioSOLVER Free (but not theoretically guaranteed to be the same)
    assert!((root_ev_oop - 95.57).abs() < 0.2);
    assert!((root_ev_ip - 66.98).abs() < 0.2);
}

/// Regression for reach extraction (`node_input_ranges`) across an ISOMORPHIC
/// chance hop.
///
/// `examples/turn_resolve.rs` only ever extracts ranges through a concrete,
/// non-isomorphic card, so the isomorphic path was untested. This proves that
/// `weights()` stays index-aligned with `private_cards()` even after the line
/// crosses an isomorphic chance node — a hand is zero in `weights()` exactly when
/// it conflicts with the actual board — so NO suit un-swap is needed when
/// reconstructing input ranges. (`assign_zero_weights` is what keeps the alignment;
/// the internal `apply_swap` only reorients node storage / the bunching arena.)
#[test]
fn node_input_ranges_across_isomorphic_chance() {
    // Two-tone flop (clubs & spades absent) and a monotone flop (three suits absent)
    // both force suit isomorphisms on the turn.
    assert_reach_survives_isomorphism("Td9d6h");
    assert_reach_survives_isomorphism("Td7d2d");
}

/// Solves a suit-symmetric (100%) game, navigates flop check/check -> an ISOMORPHIC
/// turn -> river, and asserts `weights()` stays index-aligned with `private_cards()`
/// against the actual board (a hand is zero iff board-blocked). The isomorphism relates
/// a board-blocked hand (holding the swapped turn suit) to a NON-blocked hand of a
/// different value, so a suit-swapped `weights` would fail the zero check.
///
/// A suit-asymmetric range can't be used to make a magnitude permutation visible: it
/// would suppress the isomorphism entirely, because suits are only interchangeable when
/// swap-partner hands carry identical weight. That is exactly why a stray swap among
/// non-board hands would be vacuous (same value either way) — the board-block zero
/// pattern is the meaningful evidence. Confirms no un-swap is needed in
/// `node_input_ranges`.
fn assert_reach_survives_isomorphism(flop: &str) {
    let flop_cards = flop_from_str(flop).unwrap();
    let flop_mask: u64 = flop_cards.iter().fold(0, |m, &c| m | (1u64 << c));

    let card_config = CardConfig {
        range: [Range::ones(); 2],
        flop: flop_cards,
        ..Default::default()
    };
    let tree_config = TreeConfig {
        starting_pot: 60,
        effective_stack: 200,
        ..Default::default() // no bet sizes -> check/check lines; weights are never multiplied
    };
    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(false);
    finalize(&mut game); // sets `Solved`; node_input_ranges requires it

    // Find any non-flop turn card that fires an isomorphism (independent of which suit
    // the solver canonicalizes to).
    let nav_turn = |game: &mut PostFlopGame, turn: u8| -> bool {
        if flop_mask & (1u64 << turn) != 0 {
            return false; // on the flop, not dealable
        }
        game.back_to_root();
        game.play(0); // OOP check
        game.play(0); // IP check
        assert!(game.is_chance_node());
        game.play(turn as usize); // any non-flop card is a valid (canonical or iso) chance
        game.turn_swap.is_some()
    };
    let turn_card = (0..52u8)
        .find(|&c| nav_turn(&mut game, c))
        .expect("some turn card must fire an isomorphism");
    assert!(game.turn_swap.is_some());

    // Continue to a river node: this exercises the reordered orientation `weights` must
    // survive after the isomorphic turn.
    game.play(0); // OOP check (turn)
    game.play(0); // IP check (turn)
    assert!(game.is_chance_node());
    let river_card = (0..52u8)
        .find(|&c| game.possible_cards() & (1u64 << c) != 0)
        .expect("a river card must be available");
    game.play(river_card as usize);

    let board: u64 = (1u64 << turn_card) | (1u64 << river_card);

    // Core assertion: a hand is zero in `weights()` iff it conflicts with the ACTUAL
    // board. This detects the swaps that matter: the isomorphism relates a board-blocked
    // hand (holding the swapped turn suit) to a NON-blocked hand, so a misaligned
    // (suit-swapped) `weights` would leave a nonzero weight on a blocked hand and fail.
    let mut any_nonzero = false;
    for player in 0..2 {
        let cards = game.private_cards(player);
        let w = game.weights(player);
        assert_eq!(cards.len(), w.len());
        for (&(c1, c2), &wi) in cards.iter().zip(w.iter()) {
            let blocked = board & ((1u64 << c1) | (1u64 << c2)) != 0;
            if blocked {
                assert_eq!(wi, 0.0, "board-blocked hand must have zero reach");
            } else {
                any_nonzero |= wi > 0.0;
            }
        }
    }
    assert!(any_nonzero, "the river node must be reachable by some hands");

    // The turnkey extractor must succeed and return non-empty ranges.
    let [oop, ip] = game.node_input_ranges();
    let combos = |r: &Range| -> f32 { r.raw_data().iter().sum() };
    assert!(
        combos(&oop) > 0.0 && combos(&ip) > 0.0,
        "extracted ranges must be non-empty",
    );
}

use postflop_solver::*;
use std::collections::HashMap;

pub const WEB_CASE_MAX_ITERATIONS: u32 = 1_000;
pub const WEB_CASE_OOP_PLAYER: usize = 0;

const WEB_CASE_FLOP: &str = "Ac9d2s";
const WEB_CASE_OOP_RANGE: &str = "AA-TT,AKs-AQs,AKo";
const WEB_CASE_IP_RANGE: &str = "JJ-66,AQs-AJs,KQs-KJs,QJs";
const WEB_CASE_STARTING_POT: i32 = 12;
const WEB_CASE_EFFECTIVE_STACK: i32 = 24;
const CANONICAL_WEB_CASE_CSV: &str = include_str!("../../examples/data/postflop-solver.csv");

#[derive(Debug, Clone, Copy)]
pub struct CanonicalHandResult {
    pub combos: f32,
    pub equity: f32,
    pub ev: f32,
    pub bet_freq: f32,
    pub check_freq: f32,
}

#[derive(Debug, Clone)]
pub struct SolvedHandResult {
    pub hand: String,
    pub combos: f32,
    pub equity: f32,
    pub ev: f32,
    pub bet_freq: f32,
    pub check_freq: f32,
}

#[derive(Debug, Clone)]
pub struct SolvedWebCase {
    pub final_exploitability: f32,
    pub target_exploitability: f32,
    pub hands: Vec<SolvedHandResult>,
}

pub fn build_web_case_game() -> PostFlopGame {
    let bet_sizes: BetSizeOptions = BetSizeOptions::try_from(("65%", "65%")).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: WEB_CASE_STARTING_POT,
        effective_stack: WEB_CASE_EFFECTIVE_STACK,
        rake_rate: 0.0,
        rake_cap: 0.0,
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
        range: [
            WEB_CASE_OOP_RANGE.parse().unwrap(),
            WEB_CASE_IP_RANGE.parse().unwrap(),
        ],
        flop: flop_from_str(WEB_CASE_FLOP).unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(false);
    game
}

pub fn web_case_target_exploitability() -> f32 {
    WEB_CASE_STARTING_POT as f32 * 0.005
}

pub fn solve_web_case() -> SolvedWebCase {
    let mut game = build_web_case_game();
    let target_exploitability = web_case_target_exploitability();
    let final_exploitability = solve(
        &mut game,
        WEB_CASE_MAX_ITERATIONS,
        target_exploitability,
        false,
    );
    game.cache_normalized_weights();

    let hand_names = holes_to_strings(game.private_cards(WEB_CASE_OOP_PLAYER)).unwrap();
    let num_hands = hand_names.len();
    let strategy = game.strategy();
    let equity = game.equity(WEB_CASE_OOP_PLAYER);
    let weights = game.normalized_weights(WEB_CASE_OOP_PLAYER);
    let ev = game.expected_values(WEB_CASE_OOP_PLAYER);

    assert_eq!(
        strategy.len(),
        num_hands * 2,
        "unexpected root action count for web case"
    );

    let hands = hand_names
        .into_iter()
        .enumerate()
        .map(|(index, hand)| SolvedHandResult {
            hand,
            combos: weights[index],
            equity: equity[index],
            ev: ev[index],
            check_freq: strategy[index],
            bet_freq: strategy[num_hands + index],
        })
        .collect();

    SolvedWebCase {
        final_exploitability,
        target_exploitability,
        hands,
    }
}

pub fn canonical_web_case_results() -> HashMap<String, CanonicalHandResult> {
    CANONICAL_WEB_CASE_CSV
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let columns: Vec<_> = line.split(',').collect();
            assert_eq!(columns.len(), 10, "unexpected canonical CSV format: {line}");

            (
                columns[0].to_string(),
                CanonicalHandResult {
                    combos: columns[2].parse().unwrap(),
                    equity: columns[3].parse().unwrap(),
                    ev: columns[4].parse().unwrap(),
                    bet_freq: columns[6].parse().unwrap(),
                    check_freq: columns[8].parse().unwrap(),
                },
            )
        })
        .collect()
}

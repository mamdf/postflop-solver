use postflop_solver::*;
use std::collections::HashMap;

pub const WEB_CASE_MAX_ITERATIONS: u32 = 1_000;
pub const WEB_CASE_OOP_PLAYER: usize = 0;
pub const MAX_COMBOS_DIFF: f32 = 5e-5;
pub const MAX_EQUITY_DIFF: f32 = 5e-4;
pub const MAX_EV_DIFF: f32 = 5e-3;
pub const MAX_ACTION_FREQ_DIFF: f32 = 5e-4;
pub const MAX_ACTION_EV_DIFF: f32 = 5e-3;
pub const MAX_MEAN_EV_DIFF: f32 = 1e-3;
pub const MAX_MEAN_ACTION_FREQ_DIFF: f32 = 1e-4;
pub const MAX_MEAN_ACTION_EV_DIFF: f32 = 1e-3;

const WEB_CASE_FLOP: &str = "Ac9d2s";
const WEB_CASE_OOP_RANGE: &str = "AA-TT,AKs-AQs,AKo";
const WEB_CASE_IP_RANGE: &str = "JJ-66,AQs-AJs,KQs-KJs,QJs";
const WEB_CASE_STARTING_POT: i32 = 12;
const WEB_CASE_EFFECTIVE_STACK: i32 = 24;
const ROOT_WEB_CASE_CSV: &str = include_str!("../fixtures/web/postflop-solver.csv");
const BET8_WEB_CASE_CSV: &str = include_str!("../fixtures/web/bet8.csv");
const BET8_CALL_TURN_KH_WEB_CASE_CSV: &str =
    include_str!("../fixtures/web/bet8_C_turn-Kh.csv");
const BET8_CALL_TURN_KH_CHECK_WEB_CASE_CSV: &str =
    include_str!("../fixtures/web/bet8_C_turn-Kh_X.csv");
const BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_WEB_CASE_CSV: &str =
    include_str!("../fixtures/web/bet8_C_turn-Kh_X_X_river-5s.csv");
const BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_ALLIN16_WEB_CASE_CSV: &str =
    include_str!("../fixtures/web/bet8_C_turn-Kh_X_X_river-5s_allin16.csv");

const ROOT_STEPS: [FixtureStep; 0] = [];
const BET8_STEPS: [FixtureStep; 1] = [FixtureStep::Action("Bet 8")];
const BET8_CALL_TURN_KH_STEPS: [FixtureStep; 3] = [
    FixtureStep::Action("Bet 8"),
    FixtureStep::Action("Call"),
    FixtureStep::ChanceCard("Kh"),
];
const BET8_CALL_TURN_KH_CHECK_STEPS: [FixtureStep; 4] = [
    FixtureStep::Action("Bet 8"),
    FixtureStep::Action("Call"),
    FixtureStep::ChanceCard("Kh"),
    FixtureStep::Action("Check"),
];
const BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_STEPS: [FixtureStep; 6] = [
    FixtureStep::Action("Bet 8"),
    FixtureStep::Action("Call"),
    FixtureStep::ChanceCard("Kh"),
    FixtureStep::Action("Check"),
    FixtureStep::Action("Check"),
    FixtureStep::ChanceCard("5s"),
];
const BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_ALLIN16_STEPS: [FixtureStep; 7] = [
    FixtureStep::Action("Bet 8"),
    FixtureStep::Action("Call"),
    FixtureStep::ChanceCard("Kh"),
    FixtureStep::Action("Check"),
    FixtureStep::Action("Check"),
    FixtureStep::ChanceCard("5s"),
    FixtureStep::Action("AllIn 16"),
];

#[derive(Debug, Clone, Copy)]
pub struct CanonicalWebCaseSpec {
    pub flop: &'static str,
    pub oop_range: &'static str,
    pub ip_range: &'static str,
    pub starting_pot: i32,
    pub effective_stack: i32,
    pub bet_sizes: (&'static str, &'static str),
    pub max_iterations: u32,
}

#[derive(Debug, Clone)]
pub struct CanonicalHandResult {
    pub combos: f32,
    pub equity: f32,
    pub ev: f32,
    pub action_freqs: HashMap<String, f32>,
    pub action_evs: HashMap<String, f32>,
}

#[derive(Debug, Clone)]
pub struct CanonicalWebCaseResults {
    pub action_labels: Vec<String>,
    pub hands: HashMap<String, CanonicalHandResult>,
}

#[derive(Debug, Clone)]
pub struct HandSnapshot {
    pub hand: String,
    pub combos: f32,
    pub equity: f32,
    pub ev: f32,
    pub action_freqs: Vec<f32>,
    pub action_evs: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Player,
    Chance,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureStep {
    Action(&'static str),
    ChanceCard(&'static str),
}

#[derive(Debug, Clone)]
pub struct TreeSnapshotFixture {
    pub name: &'static str,
    pub description: &'static str,
    pub csv_path: &'static str,
    pub csv: &'static str,
    pub steps: &'static [FixtureStep],
    pub expected_kind: NodeKind,
    pub expected_player: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotDiffThresholds {
    pub combos: f32,
    pub equity: f32,
    pub ev: f32,
    pub action_freq: f32,
    pub action_ev: f32,
    pub mean_ev: f32,
    pub mean_action_freq: f32,
    pub mean_action_ev: f32,
}

pub struct SolvedGame {
    pub game: PostFlopGame,
    pub final_exploitability: f32,
    pub target_exploitability: f32,
}

#[derive(Debug, Clone)]
pub struct CurrentNodeSnapshot {
    pub kind: NodeKind,
    pub current_player: Option<usize>,
    pub action_labels: Vec<String>,
    pub hands: Vec<HandSnapshot>,
}

pub const WEB_SNAPSHOT_DIFF_THRESHOLDS: SnapshotDiffThresholds = SnapshotDiffThresholds {
    combos: MAX_COMBOS_DIFF,
    equity: MAX_EQUITY_DIFF,
    ev: MAX_EV_DIFF,
    action_freq: MAX_ACTION_FREQ_DIFF,
    action_ev: MAX_ACTION_EV_DIFF,
    mean_ev: MAX_MEAN_EV_DIFF,
    mean_action_freq: MAX_MEAN_ACTION_FREQ_DIFF,
    mean_action_ev: MAX_MEAN_ACTION_EV_DIFF,
};

pub fn canonical_web_case_spec() -> CanonicalWebCaseSpec {
    CanonicalWebCaseSpec {
        flop: WEB_CASE_FLOP,
        oop_range: WEB_CASE_OOP_RANGE,
        ip_range: WEB_CASE_IP_RANGE,
        starting_pot: WEB_CASE_STARTING_POT,
        effective_stack: WEB_CASE_EFFECTIVE_STACK,
        bet_sizes: ("65%", "65%"),
        max_iterations: WEB_CASE_MAX_ITERATIONS,
    }
}

pub fn build_game(spec: &CanonicalWebCaseSpec) -> PostFlopGame {
    let bet_sizes: BetSizeOptions = BetSizeOptions::try_from(spec.bet_sizes).unwrap();

    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: spec.starting_pot,
        effective_stack: spec.effective_stack,
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
            spec.oop_range.parse().unwrap(),
            spec.ip_range.parse().unwrap(),
        ],
        flop: flop_from_str(spec.flop).unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    let action_tree = ActionTree::new(tree_config).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
    game.allocate_memory(false);
    game
}

pub fn target_exploitability(spec: &CanonicalWebCaseSpec) -> f32 {
    spec.starting_pot as f32 * 0.005
}

pub fn solve_game(spec: &CanonicalWebCaseSpec) -> SolvedGame {
    let mut game = build_game(spec);
    let target_exploitability = target_exploitability(spec);
    let final_exploitability = solve(&mut game, spec.max_iterations, target_exploitability, false);

    SolvedGame {
        game,
        final_exploitability,
        target_exploitability,
    }
}

pub fn snapshot_current_node(game: &mut PostFlopGame) -> CurrentNodeSnapshot {
    let action_labels = game
        .available_actions()
        .into_iter()
        .map(action_label)
        .collect::<Vec<_>>();

    if game.is_terminal_node() {
        return CurrentNodeSnapshot {
            kind: NodeKind::Terminal,
            current_player: None,
            action_labels,
            hands: Vec::new(),
        };
    }

    if game.is_chance_node() {
        return CurrentNodeSnapshot {
            kind: NodeKind::Chance,
            current_player: None,
            action_labels,
            hands: Vec::new(),
        };
    }

    game.cache_normalized_weights();

    let current_player = game.current_player();
    let hand_names = holes_to_strings(game.private_cards(current_player)).unwrap();
    let num_hands = hand_names.len();

    let strategy = game.strategy();
    let equity = game.equity(current_player);
    let weights = game.normalized_weights(current_player);
    let ev = game.expected_values(current_player);
    let action_evs = game.expected_values_detail(current_player);

    assert_eq!(
        strategy.len(),
        num_hands * action_labels.len(),
        "strategy/action shape mismatch at current node"
    );
    assert_eq!(
        action_evs.len(),
        num_hands * action_labels.len(),
        "action EV/action shape mismatch at current node"
    );

    let hands = hand_names
        .into_iter()
        .enumerate()
        .map(|(index, hand)| HandSnapshot {
            hand,
            combos: weights[index],
            equity: equity[index],
            ev: ev[index],
            action_freqs: (0..action_labels.len())
                .map(|action_index| strategy[action_index * num_hands + index])
                .collect(),
            action_evs: (0..action_labels.len())
                .map(|action_index| action_evs[action_index * num_hands + index])
                .collect(),
        })
        .collect();

    CurrentNodeSnapshot {
        kind: NodeKind::Player,
        current_player: Some(current_player),
        action_labels,
        hands,
    }
}

pub fn canonical_web_tree_fixtures() -> Vec<TreeSnapshotFixture> {
    vec![
        TreeSnapshotFixture {
            name: "root",
            description: "root del flop: OOP decide entre Bet 8 y Check",
            csv_path: "tests/fixtures/web/postflop-solver.csv",
            csv: ROOT_WEB_CASE_CSV,
            steps: &ROOT_STEPS,
            expected_kind: NodeKind::Player,
            expected_player: Some(WEB_CASE_OOP_PLAYER),
        },
        TreeSnapshotFixture {
            name: "oop-bet-8",
            description: "tras Bet 8 de OOP: IP responde con AllIn 24, Call o Fold",
            csv_path: "tests/fixtures/web/bet8.csv",
            csv: BET8_WEB_CASE_CSV,
            steps: &BET8_STEPS,
            expected_kind: NodeKind::Player,
            expected_player: Some(1),
        },
        TreeSnapshotFixture {
            name: "oop-bet-8_ip-call_turn-kh",
            description: "tras Bet 8, Call y turn Kh: OOP juega el turn",
            csv_path: "tests/fixtures/web/bet8_C_turn-Kh.csv",
            csv: BET8_CALL_TURN_KH_WEB_CASE_CSV,
            steps: &BET8_CALL_TURN_KH_STEPS,
            expected_kind: NodeKind::Player,
            expected_player: Some(0),
        },
        TreeSnapshotFixture {
            name: "oop-bet-8_ip-call_turn-kh_oop-check",
            description: "tras Bet 8, Call, turn Kh y Check de OOP: IP responde en turn",
            csv_path: "tests/fixtures/web/bet8_C_turn-Kh_X.csv",
            csv: BET8_CALL_TURN_KH_CHECK_WEB_CASE_CSV,
            steps: &BET8_CALL_TURN_KH_CHECK_STEPS,
            expected_kind: NodeKind::Player,
            expected_player: Some(1),
        },
        TreeSnapshotFixture {
            name: "oop-bet-8_ip-call_turn-kh_check-check_river-5s",
            description: "tras Bet 8, Call, turn Kh, check-check y river 5s: OOP juega river",
            csv_path: "tests/fixtures/web/bet8_C_turn-Kh_X_X_river-5s.csv",
            csv: BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_WEB_CASE_CSV,
            steps: &BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_STEPS,
            expected_kind: NodeKind::Player,
            expected_player: Some(0),
        },
        TreeSnapshotFixture {
            name: "oop-bet-8_ip-call_turn-kh_check-check_river-5s_oop-allin-16",
            description: "tras llegar al river 5s y AllIn 16 de OOP: IP decide entre Call y Fold",
            csv_path: "tests/fixtures/web/bet8_C_turn-Kh_X_X_river-5s_allin16.csv",
            csv: BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_ALLIN16_WEB_CASE_CSV,
            steps: &BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_ALLIN16_STEPS,
            expected_kind: NodeKind::Player,
            expected_player: Some(1),
        },
    ]
}

pub fn canonical_web_fixture_summary() -> String {
    canonical_web_tree_fixtures()
        .into_iter()
        .enumerate()
        .map(|(index, fixture)| {
            let steps = if fixture.steps.is_empty() {
                "root".to_string()
            } else {
                fixture
                    .steps
                    .iter()
                    .map(fixture_step_label)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            };

            format!(
                "{}. {} | {} | steps: {} | csv: {}",
                index + 1,
                fixture.name,
                fixture.description,
                steps,
                fixture.csv_path
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn snapshot_for_fixture(
    game: &mut PostFlopGame,
    fixture: &TreeSnapshotFixture,
) -> CurrentNodeSnapshot {
    game.back_to_root();
    navigate_to_fixture(game, fixture);
    snapshot_current_node(game)
}

pub fn assert_fixture_matches_snapshot(
    game: &mut PostFlopGame,
    fixture: &TreeSnapshotFixture,
    thresholds: &SnapshotDiffThresholds,
) {
    let snapshot = snapshot_for_fixture(game, fixture);
    let expected = parse_web_case_csv(fixture.csv);

    assert_eq!(
        snapshot.kind, fixture.expected_kind,
        "fixture '{}' resolved to {:?}, expected {:?}",
        fixture.name, snapshot.kind, fixture.expected_kind
    );
    assert_eq!(
        snapshot.current_player, fixture.expected_player,
        "fixture '{}' current player mismatch",
        fixture.name
    );
    assert_eq!(
        snapshot.action_labels.len(), expected.action_labels.len(),
        "fixture '{}' action count mismatch",
        fixture.name
    );

    for label in &expected.action_labels {
        assert!(
            snapshot.action_labels.iter().any(|candidate| candidate == label),
            "fixture '{}' missing runtime action label '{}'",
            fixture.name,
            label
        );
    }

    let actual_hands = snapshot
        .hands
        .iter()
        .filter(|hand| hand.combos > 0.0)
        .map(|hand| (hand.hand.as_str(), hand))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        actual_hands.len(),
        expected.hands.len(),
        "fixture '{}' active hand count mismatch",
        fixture.name
    );
    let mut total_ev_diff = 0.0;
    let mut total_action_freq_diff = 0.0;
    let mut total_action_ev_diff = 0.0;

    for (hand_name, expected_hand) in &expected.hands {
        let actual_hand = actual_hands.get(hand_name.as_str()).unwrap_or_else(|| {
            panic!(
                "fixture '{}' missing runtime row for hand {}",
                fixture.name, hand_name
            )
        });

        let combos_diff = (actual_hand.combos - expected_hand.combos).abs();
        let equity_diff = (actual_hand.equity - expected_hand.equity).abs();
        let ev_diff = (actual_hand.ev - expected_hand.ev).abs();
        total_ev_diff += ev_diff;

        assert!(
            combos_diff <= thresholds.combos,
            "fixture '{}' hand {} combos diff {} > {}",
            fixture.name,
            hand_name,
            combos_diff,
            thresholds.combos
        );
        assert!(
            equity_diff <= thresholds.equity,
            "fixture '{}' hand {} equity diff {} > {}",
            fixture.name,
            hand_name,
            equity_diff,
            thresholds.equity
        );
        assert!(
            ev_diff <= thresholds.ev,
            "fixture '{}' hand {} EV diff {} > {}",
            fixture.name,
            hand_name,
            ev_diff,
            thresholds.ev
        );

        for label in &expected.action_labels {
            let action_index = snapshot
                .action_labels
                .iter()
                .position(|candidate| candidate == label)
                .unwrap_or_else(|| {
                    panic!(
                        "fixture '{}' missing action label '{}' in runtime snapshot",
                        fixture.name, label
                    )
                });
            let actual_freq = actual_hand.action_freqs[action_index];
            let expected_freq = expected_hand.action_freqs.get(label).copied().unwrap_or_else(|| {
                panic!(
                    "fixture '{}' missing expected frequency for action '{}'",
                    fixture.name, label
                )
            });
            let actual_action_ev = actual_hand.action_evs[action_index];
            let expected_action_ev =
                expected_hand.action_evs.get(label).copied().unwrap_or_else(|| {
                    panic!(
                        "fixture '{}' missing expected action EV for action '{}'",
                        fixture.name, label
                    )
                });

            let action_freq_diff = (actual_freq - expected_freq).abs();
            let action_ev_diff = (actual_action_ev - expected_action_ev).abs();
            total_action_freq_diff += action_freq_diff;
            total_action_ev_diff += action_ev_diff;

            assert!(
                action_freq_diff <= thresholds.action_freq,
                "fixture '{}' hand {} action '{}' freq diff {} > {}",
                fixture.name,
                hand_name,
                label,
                action_freq_diff,
                thresholds.action_freq
            );
            assert!(
                action_ev_diff <= thresholds.action_ev,
                "fixture '{}' hand {} action '{}' EV diff {} > {}",
                fixture.name,
                hand_name,
                label,
                action_ev_diff,
                thresholds.action_ev
            );
        }
    }

    let hand_count = snapshot.hands.len() as f32;
    let action_count = expected.action_labels.len() as f32;
    let mean_ev_diff = total_ev_diff / hand_count;
    let mean_action_freq_diff = total_action_freq_diff / (hand_count * action_count);
    let mean_action_ev_diff = total_action_ev_diff / (hand_count * action_count);

    assert!(
        mean_ev_diff <= thresholds.mean_ev,
        "fixture '{}' mean EV diff {} > {}",
        fixture.name,
        mean_ev_diff,
        thresholds.mean_ev
    );
    assert!(
        mean_action_freq_diff <= thresholds.mean_action_freq,
        "fixture '{}' mean action freq diff {} > {}",
        fixture.name,
        mean_action_freq_diff,
        thresholds.mean_action_freq
    );
    assert!(
        mean_action_ev_diff <= thresholds.mean_action_ev,
        "fixture '{}' mean action EV diff {} > {}",
        fixture.name,
        mean_action_ev_diff,
        thresholds.mean_action_ev
    );
}

fn navigate_to_fixture(game: &mut PostFlopGame, fixture: &TreeSnapshotFixture) {
    for (step_index, step) in fixture.steps.iter().enumerate() {
        match step {
            FixtureStep::Action(expected_label) => {
                if game.is_terminal_node() {
                    panic!(
                        "fixture '{}' step {} expected action '{}' but current node is terminal",
                        fixture.name, step_index, expected_label
                    );
                }
                if game.is_chance_node() {
                    panic!(
                        "fixture '{}' step {} expected action '{}' but current node is chance",
                        fixture.name, step_index, expected_label
                    );
                }

                let available_actions = game.available_actions();
                let target_label = normalize_action_label(expected_label);
                let action_index = available_actions
                    .iter()
                    .position(|action| action_label(*action) == target_label)
                    .unwrap_or_else(|| {
                        panic!(
                            "fixture '{}' step {} action '{}' not found. Available: {:?}",
                            fixture.name,
                            step_index,
                            expected_label,
                            available_actions
                                .iter()
                                .map(|action| action_label(*action))
                                .collect::<Vec<_>>()
                        )
                    });

                game.play(action_index);
            }
            FixtureStep::ChanceCard(card_label) => {
                if !game.is_chance_node() {
                    panic!(
                        "fixture '{}' step {} expected chance card '{}' but current node is not chance",
                        fixture.name, step_index, card_label
                    );
                }

                let card = card_from_str(card_label).unwrap_or_else(|error| {
                    panic!(
                        "fixture '{}' step {} has invalid chance card '{}': {}",
                        fixture.name, step_index, card_label, error
                    )
                });
                let possible_cards = game.possible_cards();
                assert!(
                    (possible_cards & (1u64 << card)) != 0,
                    "fixture '{}' step {} chance card '{}' is not legal at this node",
                    fixture.name,
                    step_index,
                    card_label
                );

                game.play(card as usize);
            }
        }
    }
}

fn parse_web_case_csv(csv: &str) -> CanonicalWebCaseResults {
    let mut lines = csv.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .expect("web case CSV must include a header row");
    let columns = header.split(',').map(str::trim).collect::<Vec<_>>();

    let mut hand_index = None;
    let mut combos_index = None;
    let mut equity_index = None;
    let mut ev_index = None;
    let mut action_labels = Vec::new();
    let mut action_columns: HashMap<String, (Option<usize>, Option<usize>)> = HashMap::new();

    for (index, column) in columns.iter().enumerate() {
        match *column {
            "Hand" => hand_index = Some(index),
            "Combos" => combos_index = Some(index),
            "Equity" => equity_index = Some(index),
            "EV" => ev_index = Some(index),
            _ => {
                if let Some(label) = column.strip_suffix(" %") {
                    let normalized = normalize_action_label(label);
                    let entry = action_columns.entry(normalized.clone()).or_insert_with(|| {
                        action_labels.push(normalized.clone());
                        (None, None)
                    });
                    entry.0 = Some(index);
                } else if let Some(label) = column.strip_suffix(" EV") {
                    let normalized = normalize_action_label(label);
                    let entry = action_columns.entry(normalized.clone()).or_insert_with(|| {
                        action_labels.push(normalized.clone());
                        (None, None)
                    });
                    entry.1 = Some(index);
                }
            }
        }
    }

    let hand_index = hand_index.expect("web case CSV missing Hand column");
    let combos_index = combos_index.expect("web case CSV missing Combos column");
    let equity_index = equity_index.expect("web case CSV missing Equity column");
    let ev_index = ev_index.expect("web case CSV missing EV column");

    let action_layout = action_labels
        .iter()
        .map(|label| {
            let (freq_index, ev_action_index) = action_columns
                .get(label)
                .copied()
                .unwrap_or_else(|| panic!("missing action layout for {label}"));

            (
                label.clone(),
                freq_index.unwrap_or_else(|| panic!("missing action frequency column for {label}")),
                ev_action_index.unwrap_or_else(|| panic!("missing action EV column for {label}")),
            )
        })
        .collect::<Vec<_>>();

    let hands = lines
        .map(|line| {
            let columns = line.split(',').map(str::trim).collect::<Vec<_>>();
            assert_eq!(
                columns.len(),
                header.split(',').count(),
                "unexpected web case CSV format: {line}"
            );
            let action_freqs = action_layout
                .iter()
                .map(|(label, freq_index, _)| {
                    (label.clone(), parse_f32(columns[*freq_index], line))
                })
                .collect();
            let action_evs = action_layout
                .iter()
                .map(|(label, _, action_ev_index)| {
                    (label.clone(), parse_f32(columns[*action_ev_index], line))
                })
                .collect();

            (
                columns[hand_index].to_string(),
                CanonicalHandResult {
                    combos: parse_f32(columns[combos_index], line),
                    equity: parse_f32(columns[equity_index], line),
                    ev: parse_f32(columns[ev_index], line),
                    action_freqs,
                    action_evs,
                },
            )
        })
        .collect();

    CanonicalWebCaseResults {
        action_labels,
        hands,
    }
}

fn parse_f32(value: &str, line: &str) -> f32 {
    value
        .parse()
        .unwrap_or_else(|error| panic!("failed to parse float '{value}' in line '{line}': {error}"))
}

fn normalize_action_label(label: &str) -> String {
    let mut parts = label.split_whitespace();
    let Some(verb) = parts.next() else {
        return String::new();
    };

    let normalized_verb = match &*verb.to_ascii_lowercase() {
        "allin" => "AllIn",
        "bet" => "Bet",
        "raise" => "Raise",
        "check" => "Check",
        "call" => "Call",
        "fold" => "Fold",
        "chance" => "Chance",
        "none" => "None",
        _ => verb,
    };
    let suffix = parts.collect::<Vec<_>>().join(" ");

    if suffix.is_empty() {
        normalized_verb.to_string()
    } else {
        format!("{normalized_verb} {suffix}")
    }
}

fn action_label(action: Action) -> String {
    match action {
        Action::None => "None".to_string(),
        Action::Fold => "Fold".to_string(),
        Action::Check => "Check".to_string(),
        Action::Call => "Call".to_string(),
        Action::Bet(amount) => format!("Bet {amount}"),
        Action::Raise(amount) => format!("Raise {amount}"),
        Action::AllIn(amount) => format!("AllIn {amount}"),
        Action::Chance(card) => card_to_string(card)
            .map(|label| format!("Chance {label}"))
            .unwrap_or_else(|_| format!("Chance {card}")),
    }
}

fn fixture_step_label(step: &FixtureStep) -> String {
    match step {
        FixtureStep::Action(label) => format!("action:{label}"),
        FixtureStep::ChanceCard(card) => format!("chance:{card}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_web_fixture_actions_from_header() {
        let parsed = parse_web_case_csv(ROOT_WEB_CASE_CSV);

        assert_eq!(parsed.action_labels, vec!["Bet 8", "Check"]);

        let aces = parsed.hands.get("AsAh").unwrap();
        assert_eq!(aces.action_freqs["Bet 8"], 0.229991);
        assert_eq!(aces.action_freqs["Check"], 0.770009);
        assert_eq!(aces.action_evs["Bet 8"], 15.2181);
        assert_eq!(aces.action_evs["Check"], 15.2073);
    }

    #[test]
    fn normalizes_allin_labels_and_preserves_header_order() {
        let parsed = parse_web_case_csv(BET8_WEB_CASE_CSV);

        assert_eq!(parsed.action_labels, vec!["AllIn 24", "Call", "Fold"]);

        let hand = parsed.hands.get("AsQs").unwrap();
        assert_eq!(hand.action_freqs["AllIn 24"], 0.605865);
        assert_eq!(hand.action_evs["AllIn 24"], 8.70938);
        assert_eq!(hand.action_freqs["Call"], 0.394135);
        assert_eq!(hand.action_freqs["Fold"], 0.0);
    }

    #[test]
    fn parses_call_fold_web_fixture_action_evs() {
        let parsed = parse_web_case_csv(BET8_CALL_TURN_KH_CHECK_CHECK_RIVER_5S_ALLIN16_WEB_CASE_CSV);

        assert_eq!(parsed.action_labels, vec!["Call", "Fold"]);

        let hand = parsed.hands.get("QsJs").unwrap();
        assert_eq!(hand.action_freqs["Call"], 0.0);
        assert_eq!(hand.action_evs["Call"], -16.000004);
        assert_eq!(hand.action_freqs["Fold"], 1.0);
        assert_eq!(hand.action_evs["Fold"], 0.0);
    }

    #[test]
    fn canonical_web_fixture_registry_uses_explicit_navigation() {
        let fixtures = canonical_web_tree_fixtures();

        assert_eq!(fixtures.len(), 6);
        assert_eq!(fixtures[0].steps, &[]);
        assert_eq!(fixtures[1].steps, &[FixtureStep::Action("Bet 8")]);
        assert_eq!(
            fixtures[5].steps,
            &[
                FixtureStep::Action("Bet 8"),
                FixtureStep::Action("Call"),
                FixtureStep::ChanceCard("Kh"),
                FixtureStep::Action("Check"),
                FixtureStep::Action("Check"),
                FixtureStep::ChanceCard("5s"),
                FixtureStep::Action("AllIn 16"),
            ]
        );
    }
}

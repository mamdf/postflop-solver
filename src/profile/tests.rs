use super::*;
use crate::bet_size::*;

// Small turn spot on a two-tone flop: clubs and diamonds are isomorphic on the river.
fn test_game() -> PostFlopGame {
    let card_config = CardConfig {
        range: [
            "AA,KK,QQ,AK,T9s".parse().unwrap(),
            "AA,KK,JJ,AQ,98s".parse().unwrap(),
        ],
        flop: flop_from_str("Ks7h2h").unwrap(),
        turn: card_from_str("4s").unwrap(),
        river: NOT_DEALT,
    };
    let turn = BetSizeOptions::try_from(("50%", "60%")).unwrap();
    let river = BetSizeOptions::try_from(("75%, a", "2x")).unwrap();
    let tree_config = TreeConfig {
        initial_state: BoardState::Turn,
        starting_pot: 100,
        effective_stack: 200,
        turn_bet_sizes: [turn.clone(), turn],
        river_bet_sizes: [river.clone(), river],
        ..Default::default()
    };
    let mut game =
        PostFlopGame::with_config(card_config, ActionTree::new(tree_config).unwrap()).unwrap();
    game.allocate_memory(false);
    game
}

/// Solved profile and its own exploitability.
fn solved_profile() -> (StrategyProfile, f32) {
    let mut game = test_game();
    let expl = solve(&mut game, 100, 0.0, false);
    (StrategyProfile::from_game(&mut game, "self").unwrap(), expl)
}

fn evaluate(profile: &dyn StrategySource, matching: MatchOptions) -> ProfileEvaluation {
    let opts = EvaluateOptions {
        matching,
        fill_iterations: 0,
    };
    evaluate_source(&mut test_game(), profile, opts).unwrap()
}

/// Rebuilds `p` with every action key (nodes and lines) mapped by `f` and the pot scaled.
fn map_actions(
    p: &StrategyProfile,
    pot_scale: f64,
    f: impl Fn(ActionKey) -> ActionKey,
) -> StrategyProfile {
    let hands = [p.hands(0).to_vec(), p.hands(1).to_vec()];
    let mut out = StrategyProfile::new("mapped", p.starting_pot * pot_scale, hands).unwrap();
    for (i, n) in p.nodes().iter().enumerate() {
        let line = n
            .line
            .iter()
            .map(|s| match *s {
                LineStep::Action(a) => LineStep::Action(f(a)),
                card => card,
            })
            .collect();
        let actions = n.actions.iter().map(|&a| f(a)).collect();
        out.push_node(line, n.player, actions, p.node_strategy(i))
            .unwrap();
    }
    out
}

fn scale_key(a: ActionKey, k: f64) -> ActionKey {
    match a {
        ActionKey::Bet(x) => ActionKey::Bet(x * k),
        ActionKey::Raise(x) => ActionKey::Raise(x * k),
        ActionKey::AllIn(x) => ActionKey::AllIn(x * k),
        other => other,
    }
}

#[test]
fn profile_self_relock() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    let (profile, expl) = solved_profile();
    let ev = evaluate(&profile, MatchOptions::default());
    assert!(ev.complete, "{:?}", ev.report);
    assert!((ev.exploitability - expl).abs() < 1e-3 * 100.0);
    assert!((ev.exploitability_pct - expl).abs() < 1e-1); // pot = 100
    assert!(ev
        .report
        .players
        .iter()
        .all(|p| p.locked == p.action_nodes && p.locked > 0));
}

#[test]
fn profile_uniform_is_more_exploitable() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    let (_, expl) = solved_profile();
    let uniform = StrategyProfile::from_game(&mut test_game(), "uniform").unwrap();
    let ev = evaluate(&uniform, MatchOptions::default());
    assert!(ev.complete);
    assert!(
        ev.exploitability > expl + 1.0,
        "{} vs {expl}",
        ev.exploitability
    );
}

#[test]
fn profile_order_independent() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    let (profile, _) = solved_profile();
    let rev_hands = [0, 1].map(|p| profile.hands(p).iter().rev().copied().collect::<Vec<_>>());
    let mut reversed = StrategyProfile::new("reversed", profile.starting_pot, rev_hands).unwrap();
    for (i, n) in profile.nodes().iter().enumerate() {
        let (na, nh) = (n.actions.len(), profile.hands(n.player).len());
        let s = profile.node_strategy(i);
        let r: Vec<f32> = (0..na * nh)
            .map(|k| s[(na - 1 - k / nh) * nh + (nh - 1 - k % nh)])
            .collect();
        let actions = n.actions.iter().rev().copied().collect();
        reversed
            .push_node(n.line.clone(), n.player, actions, &r)
            .unwrap();
    }
    let a = evaluate(&profile, MatchOptions::default());
    let b = evaluate(&reversed, MatchOptions::default());
    assert!(b.complete);
    assert!((a.exploitability - b.exploitability).abs() < 1e-4 * 100.0);
}

#[test]
fn profile_amount_scale() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    let (profile, expl) = solved_profile();
    let scaled = map_actions(&profile, 100.0, |a| scale_key(a, 100.0));
    let ev = evaluate(&scaled, MatchOptions::default());
    assert!(ev.complete);
    assert!((ev.exploitability - expl).abs() < 1e-3 * 100.0);

    let wrong = MatchOptions {
        amount_scale: Some(1.0),
        ..Default::default()
    };
    let ev = evaluate(&scaled, wrong);
    assert!(!ev.complete);
    assert!(ev.report.players.iter().any(|p| p.missing > 0));
    assert!(ev.report.players[0].mismatched >= 1); // stray bet mass at the root
}

#[test]
fn profile_root_only_and_nan_rows() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    let mut game = test_game();
    let actions: Vec<ActionKey> = game
        .available_actions()
        .into_iter()
        .filter_map(ActionKey::from_action)
        .collect();
    let mut hands = [0, 1].map(|p| game.private_cards(p).to_vec());
    hands[0].pop(); // last OOP hand unmapped
    let nh = hands[0].len();
    let mut strategy = vec![1.0; actions.len() * nh];
    for a in 0..actions.len() {
        strategy[a * nh] = f32::NAN; // first OOP hand left free
    }
    let mut profile = StrategyProfile::new("root", 100.0, hands.clone()).unwrap();
    assert!(profile
        .push_node(Vec::new(), 0, actions.clone(), &strategy[1..])
        .is_err());
    profile
        .push_node(Vec::new(), 0, actions.clone(), &strategy)
        .unwrap();

    let report = lock_source(&mut game, &profile, LockOptions::default()).unwrap();
    let [oop, ip] = report.players;
    assert_eq!((oop.locked, ip.locked), (1, 0));
    assert_eq!(oop.missing, oop.action_nodes - 1);
    assert_eq!(ip.missing, ip.action_nodes);
    assert_eq!(
        (oop.free_hands, oop.unmapped_hands, oop.mismatched),
        (1, 1, 0)
    );
    assert!(!report.is_complete() && !report.issues.is_empty());
    assert!(game.current_locking_strategy().is_some());

    // CFR-filling the uncovered parts makes the profile less exploitable
    let fill = |fill_iterations| {
        let opts = EvaluateOptions {
            fill_iterations,
            ..Default::default()
        };
        evaluate_source(&mut test_game(), &profile, opts)
            .unwrap()
            .exploitability
    };
    assert!(fill(20) < fill(0));

    // a root node of the other player is a player mismatch, not missing
    let mut flipped = StrategyProfile::new("flipped", 100.0, hands.clone()).unwrap();
    let ip_strategy = vec![1.0; actions.len() * hands[1].len()];
    flipped
        .push_node(Vec::new(), 1, actions, &ip_strategy)
        .unwrap();
    let report = lock_source(&mut game, &flipped, LockOptions::default()).unwrap();
    assert_eq!(report.players[0].mismatched, 1);
    assert!(report.issues[0].starts_with("OOP player mismatch"));
}

#[test]
fn profile_small_chip_sizes() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    // river spot in bb units: root actions are [Check, Bet(2), Bet(3)], one chip apart
    let card_config = CardConfig {
        range: [
            "AA,KK,QQ,AK".parse().unwrap(),
            "KK,QQ,JJ,AQ".parse().unwrap(),
        ],
        flop: flop_from_str("Ks7h2h").unwrap(),
        turn: card_from_str("4s").unwrap(),
        river: card_from_str("9c").unwrap(),
    };
    let sizes = BetSizeOptions::try_from(("33%, 50%", "")).unwrap();
    let tree_config = TreeConfig {
        initial_state: BoardState::River,
        starting_pot: 6,
        effective_stack: 97,
        river_bet_sizes: [sizes.clone(), sizes],
        ..Default::default()
    };
    let new_game = || {
        let tree = ActionTree::new(tree_config.clone()).unwrap();
        let mut game = PostFlopGame::with_config(card_config.clone(), tree).unwrap();
        game.allocate_memory(false);
        game
    };
    let mut game = new_game();
    assert_eq!(
        game.available_actions(),
        vec![Action::Check, Action::Bet(2), Action::Bet(3)]
    );
    let expl = solve(&mut game, 100, 0.0, false);
    let profile = StrategyProfile::from_game(&mut game, "self").unwrap();

    let ev = evaluate_source(&mut new_game(), &profile, EvaluateOptions::default()).unwrap();
    assert!(ev.complete, "{:?}", ev.report);
    assert!((ev.exploitability - expl).abs() < 1e-3 * 6.0);

    let mut game = new_game();
    let opts = MatchOptions::default();
    let line = |t| [LineStep::from_token(t).unwrap()];
    assert_eq!(resolve_line(&mut game, &line("B:2"), &opts), Ok(vec![1]));
    assert_eq!(resolve_line(&mut game, &line("B:3.2"), &opts), Ok(vec![2]));
    assert!(resolve_line(&mut game, &line("B:2.5"), &opts).is_err()); // equally close
    let actions = game.available_actions();
    assert_eq!(match_action(&actions, ActionKey::Check, &opts), Some(0));
    assert_eq!(
        match_action(&actions, ActionKey::Raise(3.0), &opts),
        Some(2)
    );
    assert_eq!(match_action(&actions, ActionKey::Fold, &opts), None);
    assert_eq!(match_action(&actions, ActionKey::Bet(2.5), &opts), None);

    // a zero-mass source action colliding with one of ours is harmless; with mass, mismatched
    let root = profile.node_strategy(profile.find(&[]).unwrap());
    let nh = profile.hands(0).len();
    let bet = (1..3)
        .max_by(|&a, &b| {
            let m = |a: usize| root[a * nh..(a + 1) * nh].iter().sum::<f32>();
            m(a).total_cmp(&m(b))
        })
        .unwrap();
    let extra = ActionKey::Bet(profile.nodes()[0].actions[bet].amount().unwrap() + 0.2);
    let with_extra_root_action = |mass: f32| {
        let hands = [profile.hands(0).to_vec(), profile.hands(1).to_vec()];
        let mut out = StrategyProfile::new("extra", 6.0, hands).unwrap();
        for (i, n) in profile.nodes().iter().enumerate() {
            let (mut actions, mut s) = (n.actions.clone(), profile.node_strategy(i).to_vec());
            if n.line.is_empty() {
                actions.push(extra);
                s.extend(vec![mass; nh]);
            }
            out.push_node(n.line.clone(), n.player, actions, &s)
                .unwrap();
        }
        out
    };
    let ev = evaluate_source(
        &mut new_game(),
        &with_extra_root_action(0.0),
        EvaluateOptions::default(),
    )
    .unwrap();
    assert!(ev.complete, "{:?}", ev.report);
    assert!((ev.exploitability - expl).abs() < 1e-3 * 6.0);
    let report = lock_source(
        &mut new_game(),
        &with_extra_root_action(1.0),
        LockOptions::default(),
    )
    .unwrap();
    assert_eq!(report.players[0].mismatched, 1);
}

#[test]
fn profile_aggressive_class_and_resolve_line() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    let (profile, expl) = solved_profile();
    let swapped = map_actions(&profile, 1.0, |a| match a {
        ActionKey::Bet(x) => ActionKey::Raise(x),
        ActionKey::Raise(x) => ActionKey::Bet(x),
        ActionKey::AllIn(x) => ActionKey::AllIn(x + 37.0),
        other => other,
    });
    let ev = evaluate(&swapped, MatchOptions::default());
    assert!(ev.complete, "{:?}", ev.report);
    assert!((ev.exploitability - expl).abs() < 1e-3 * 100.0);

    // OOP bets 50 (typed as a raise), IP calls, river 9c (isomorphic to 9d), OOP checks
    let mut game = test_game();
    let line: Vec<LineStep> = ["R:50", "C", "9c", "X"]
        .iter()
        .map(|t| LineStep::from_token(t).unwrap())
        .collect();
    let history = resolve_line(&mut game, &line, &MatchOptions::default()).unwrap();
    assert_eq!(
        history,
        vec![1, 1, card_from_str("9c").unwrap() as usize, 0]
    );
    let bad = [LineStep::Action(ActionKey::Bet(70.0))];
    assert!(resolve_line(&mut game, &bad, &MatchOptions::default()).is_err());
}

#[cfg(feature = "profile-io")]
#[test]
fn profile_dir_round_trip() {
    // budget: <0.1s @ RAYON_NUM_THREADS=4 (warm)
    use serde_json::json;
    use std::io::Write;

    let (mut profile, _) = solved_profile();
    profile.data[0] = f32::NAN;
    let same = |a: &StrategyProfile, b: &StrategyProfile| {
        assert_eq!(a.hands, b.hands);
        assert_eq!(a.nodes().len(), b.nodes().len());
        for (i, n) in a.nodes().iter().enumerate() {
            let j = b.find(&n.line).unwrap();
            assert_eq!(
                (n.player, &n.actions),
                (b.nodes()[j].player, &b.nodes()[j].actions)
            );
            let (x, y) = (a.node_strategy(i), b.node_strategy(j));
            assert!(x
                .iter()
                .zip(y)
                .all(|(p, q)| p == q || p.is_nan() && q.is_nan()));
        }
    };
    let base = std::env::temp_dir().join(format!("pfs-profile-{}", std::process::id()));

    // version 2
    let v2 = base.join("v2");
    dir::write_dir(&profile, &v2).unwrap();
    let read = dir::read_dir(&v2).unwrap();
    assert_eq!((read.source.as_str(), read.starting_pot), ("self", 100.0));
    same(&profile, &read);

    // version 1: index paths, card-id hands; strategy file is shared with v2
    let v1 = base.join("v1");
    std::fs::create_dir_all(&v1).unwrap();
    std::fs::copy(v2.join("strat.f32"), v1.join("strat.f32")).unwrap();
    let hands = [0, 1].map(|p| {
        profile
            .hands(p)
            .iter()
            .map(|&(a, b)| [b, a])
            .collect::<Vec<_>>()
    });
    let header = json!({"source": "v1", "starting_pot": 100.0, "hands": hands});
    std::fs::write(v1.join("header.json"), header.to_string()).unwrap();
    let mut nodes = std::fs::File::create(v1.join("nodes.jsonl")).unwrap();
    for n in profile.nodes() {
        let tokens: Vec<String> = (0..n.line.len())
            .map(|k| match n.line[k] {
                LineStep::Card(c) => format!("c{c}"),
                LineStep::Action(a) => {
                    let parent = &profile.nodes()[profile.find(&n.line[..k]).unwrap()];
                    parent
                        .actions
                        .iter()
                        .position(|&b| b == a)
                        .unwrap()
                        .to_string()
                }
            })
            .collect();
        let actions: Vec<String> = n.actions.iter().map(ActionKey::to_token).collect();
        let rec = json!({"path": tokens.join("/"), "player": n.player, "actions": actions, "offset": n.offset});
        writeln!(nodes, "{rec}").unwrap();
    }
    drop(nodes);
    same(&profile, &dir::read_dir(&v1).unwrap());

    std::fs::remove_dir_all(&base).unwrap();
}

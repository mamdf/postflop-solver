//! Solver-agnostic strategy profiles: lock a foreign strategy into a [`PostFlopGame`] and measure
//! it with this solver's best response.
//!
//! - A source of strategies implements [`StrategySource`]; [`StrategyProfile`] is the dense
//!   implementation included here (see `profile::dir` with the `profile-io` feature for its file
//!   format).
//! - Lines are typed ([`LineStep`]) and use a single amount convention: Bet/Raise/AllIn amounts
//!   are "raise-to" on the current street, in chips, as in [`Action`].
//! - Our actions are matched to the source's by kind and (scaled) amount, see [`MatchOptions`].

use crate::action_tree::*;
use crate::card::*;
use crate::game::*;
use crate::interface::*;
use crate::range::*;
use crate::solver::*;
use crate::utility::*;
use std::collections::HashMap;

#[cfg(feature = "profile-io")]
pub mod dir;

#[cfg(test)]
mod tests;

/// Maximum number of messages kept in [`LockReport::issues`].
const MAX_ISSUES: usize = 20;

/// Hole cards as `(low_id, high_id)`.
pub type Hand = (Card, Card);

/// Parses a 4-character hand such as `"AsKd"`; the card order does not matter.
pub fn parse_hand(s: &str) -> Result<Hand, String> {
    let mut chars = s.chars();
    let a = card_from_chars(&mut chars)?;
    let b = card_from_chars(&mut chars)?;
    if chars.next().is_some() || a == b {
        return Err(format!("Invalid hand: {s:?}"));
    }
    Ok((a.min(b), a.max(b)))
}

/// Normalized action of a strategy source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionKey {
    Fold,
    Check,
    Call,
    /// Bet to the given street total (chips).
    Bet(f64),
    /// Raise to the given street total (chips).
    Raise(f64),
    /// All-in to the given street total (chips).
    AllIn(f64),
}

impl ActionKey {
    /// Converts a player action of the game tree; `None` for chance/none actions.
    pub fn from_action(action: Action) -> Option<Self> {
        match action {
            Action::Fold => Some(Self::Fold),
            Action::Check => Some(Self::Check),
            Action::Call => Some(Self::Call),
            Action::Bet(x) => Some(Self::Bet(x as f64)),
            Action::Raise(x) => Some(Self::Raise(x as f64)),
            Action::AllIn(x) => Some(Self::AllIn(x as f64)),
            Action::None | Action::Chance(_) => None,
        }
    }

    /// The raise-to amount of an aggressive action.
    pub fn amount(&self) -> Option<f64> {
        match *self {
            Self::Bet(x) | Self::Raise(x) | Self::AllIn(x) => Some(x),
            _ => None,
        }
    }

    /// Token form: `F`, `X`, `C`, `B:<amt>`, `R:<amt>`, `A:<amt>`.
    pub fn to_token(&self) -> String {
        match self {
            Self::Fold => "F".into(),
            Self::Check => "X".into(),
            Self::Call => "C".into(),
            Self::Bet(x) => format!("B:{x}"),
            Self::Raise(x) => format!("R:{x}"),
            Self::AllIn(x) => format!("A:{x}"),
        }
    }

    /// Parses the token form of [`to_token`](Self::to_token).
    pub fn from_token(s: &str) -> Result<Self, String> {
        let amount = |v: &str| match v.parse::<f64>() {
            Ok(x) if x.is_finite() && x >= 0.0 => Ok(x),
            _ => Err(format!("Invalid action amount: {s:?}")),
        };
        match s.split_once(':') {
            None if s == "F" => Ok(Self::Fold),
            None if s == "X" => Ok(Self::Check),
            None if s == "C" => Ok(Self::Call),
            Some(("B", v)) => Ok(Self::Bet(amount(v)?)),
            Some(("R", v)) => Ok(Self::Raise(amount(v)?)),
            Some(("A", v)) => Ok(Self::AllIn(amount(v)?)),
            _ => Err(format!("Invalid action token: {s:?}")),
        }
    }
}

/// One step of a line from the root: a player action or a dealt card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineStep {
    Action(ActionKey),
    Card(Card),
}

impl LineStep {
    /// Token form: an action token or a card such as `7h`.
    pub fn to_token(&self) -> String {
        match self {
            Self::Action(a) => a.to_token(),
            Self::Card(c) => card_to_string(*c).unwrap_or_else(|_| format!("?{c}")),
        }
    }

    /// Parses the token form of [`to_token`](Self::to_token).
    pub fn from_token(s: &str) -> Result<Self, String> {
        match ActionKey::from_token(s) {
            Ok(a) => Ok(Self::Action(a)),
            Err(_) => card_from_str(s)
                .map(Self::Card)
                .map_err(|_| format!("Invalid line token: {s:?}")),
        }
    }
}

/// Line as text (tokens joined by `/`), for messages.
fn line_key(line: &[LineStep]) -> String {
    let tokens: Vec<String> = line.iter().map(LineStep::to_token).collect();
    tokens.join("/")
}

/// Hashable form of a line (amounts by their bits).
fn index_key(line: &[LineStep]) -> Vec<(u8, u64)> {
    let bits = |x: f64| (x + 0.0).to_bits(); // -0.0 -> 0.0
    line.iter()
        .map(|s| match *s {
            LineStep::Card(c) => (0, c as u64),
            LineStep::Action(ActionKey::Fold) => (1, 0),
            LineStep::Action(ActionKey::Check) => (2, 0),
            LineStep::Action(ActionKey::Call) => (3, 0),
            LineStep::Action(ActionKey::Bet(x)) => (4, bits(x)),
            LineStep::Action(ActionKey::Raise(x)) => (5, bits(x)),
            LineStep::Action(ActionKey::AllIn(x)) => (6, bits(x)),
        })
        .collect()
}

/// Strategy of one source node.
pub struct SourceNode<'a> {
    /// Player to act at the node (a different player than ours is reported as a mismatch).
    pub player: usize,
    /// Source actions, in the order of the frequencies written by `dist`.
    pub actions: Vec<ActionKey>,
    /// `dist(hand, out)`: writes the frequencies of `hand` into `out` (one per action); `false`
    /// if the source does not know the hand (unmapped). A row with a NaN (unwritten entries are
    /// NaN) or without a positive value leaves the hand free.
    #[allow(clippy::type_complexity)]
    pub dist: Box<dyn Fn(Hand, &mut [f32]) -> bool + 'a>,
}

/// A strategy of another solver (dense dump, bucketed blueprint, ...).
pub trait StrategySource {
    /// Starting pot in the source's chips; sets the default amount scale (ours / theirs).
    fn starting_pot(&self) -> Option<f64> {
        None
    }

    /// The node reached by `line` (in the source's own action keys), `None` if not covered.
    fn node(&self, line: &[LineStep]) -> Option<SourceNode<'_>>;
}

/// A node of a [`StrategyProfile`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileNode {
    pub line: Vec<LineStep>,
    pub player: usize,
    pub actions: Vec<ActionKey>,
    offset: usize,
}

/// Dense strategy profile: per node, `#actions * #hands` frequencies (layout
/// `[action * #hands + hand]`, NaN = not covered).
#[derive(Debug, Clone, Default)]
pub struct StrategyProfile {
    pub source: String,
    pub starting_pot: f64,
    hands: [Vec<Hand>; 2],
    hand_index: [Vec<u32>; 2],
    nodes: Vec<ProfileNode>,
    index: HashMap<Vec<(u8, u64)>, usize>,
    data: Vec<f32>,
}

const NO_HAND: u32 = u32::MAX;

impl StrategyProfile {
    /// Creates an empty profile with the hand table of each player.
    pub fn new(source: &str, starting_pot: f64, hands: [Vec<Hand>; 2]) -> Result<Self, String> {
        if !(starting_pot.is_finite() && starting_pot > 0.0) {
            return Err(format!("Invalid starting pot: {starting_pot}"));
        }
        let mut hand_index = [vec![NO_HAND; 52 * 51 / 2], vec![NO_HAND; 52 * 51 / 2]];
        let hands: [Vec<Hand>; 2] = [0, 1].map(|p| {
            hands[p]
                .iter()
                .map(|&(a, b)| (a.min(b), a.max(b)))
                .collect()
        });
        for (p, list) in hands.iter().enumerate() {
            for (i, &h) in list.iter().enumerate() {
                if h.1 >= 52 || h.0 == h.1 {
                    return Err(format!("Invalid hand: {h:?}"));
                }
                let slot = &mut hand_index[p][card_pair_to_index(h.0, h.1)];
                if *slot != NO_HAND {
                    return Err(format!("Duplicate hand: {h:?}"));
                }
                *slot = i as u32;
            }
        }
        Ok(Self {
            source: source.into(),
            starting_pot,
            hands,
            hand_index,
            ..Default::default()
        })
    }

    /// Hand table of `player`.
    pub fn hands(&self, player: usize) -> &[Hand] {
        &self.hands[player]
    }

    /// All nodes, in insertion order.
    pub fn nodes(&self) -> &[ProfileNode] {
        &self.nodes
    }

    /// Strategy of the `index`-th node (layout `[action * #hands + hand]`).
    pub fn node_strategy(&self, index: usize) -> &[f32] {
        let n = &self.nodes[index];
        let len = n.actions.len() * self.hands[n.player].len();
        &self.data[n.offset..n.offset + len]
    }

    /// Index of the node reached by `line`.
    pub fn find(&self, line: &[LineStep]) -> Option<usize> {
        self.index.get(&index_key(line)).copied()
    }

    /// Adds a node; `strategy` has the layout `[action * #hands + hand]`.
    pub fn push_node(
        &mut self,
        line: Vec<LineStep>,
        player: usize,
        actions: Vec<ActionKey>,
        strategy: &[f32],
    ) -> Result<(), String> {
        if player > 1 || strategy.len() != actions.len() * self.hands[player].len() {
            return Err(format!(
                "Strategy of node {:?} does not match its actions and hands",
                line_key(&line)
            ));
        }
        let offset = self.data.len();
        self.data.extend_from_slice(strategy);
        let node = ProfileNode {
            line,
            player,
            actions,
            offset,
        };
        self.insert_node(node)
            .inspect_err(|_| self.data.truncate(offset))
    }

    /// Validates and indexes a node whose strategy is already in `data`.
    fn insert_node(&mut self, node: ProfileNode) -> Result<(), String> {
        let err = |what| Err(format!("{what} {:?}", line_key(&node.line)));
        if node.player > 1 || node.actions.is_empty() {
            return err("Invalid node");
        }
        let len = node.actions.len() * self.hands[node.player].len();
        if node.offset + len > self.data.len() {
            return err("Strategy out of bounds at node");
        }
        if self
            .index
            .insert(index_key(&node.line), self.nodes.len())
            .is_some()
        {
            return err("Duplicate node");
        }
        self.nodes.push(node);
        Ok(())
    }

    /// Exports the current strategy of `game` (every dealt card, not only representatives).
    /// Hands that overlap the board are stored as NaN.
    pub fn from_game(game: &mut PostFlopGame, source: &str) -> Result<Self, String> {
        if !game.has_full_storage() {
            return Err("Game must have memory allocated with full storage".into());
        }
        let hands = [0, 1].map(|p| game.private_cards(p).to_vec());
        let pot = game.tree_config().starting_pot as f64;
        let mut profile = Self::new(source, pot, hands)?;
        let ret = export_walk(game, &mut Vec::new(), &mut Vec::new(), &mut profile);
        game.back_to_root();
        ret.map(|_| profile)
    }
}

impl StrategySource for StrategyProfile {
    fn starting_pot(&self) -> Option<f64> {
        Some(self.starting_pot)
    }

    fn node(&self, line: &[LineStep]) -> Option<SourceNode<'_>> {
        let index = self.find(line)?;
        let node = &self.nodes[index];
        let num_hands = self.hands[node.player].len();
        let strategy = self.node_strategy(index);
        let hand_index = &self.hand_index[node.player];
        Some(SourceNode {
            player: node.player,
            actions: node.actions.clone(),
            dist: Box::new(
                move |h, out| match hand_index[card_pair_to_index(h.0, h.1)] {
                    NO_HAND => false,
                    j => {
                        for (a, v) in out.iter_mut().enumerate() {
                            *v = strategy[a * num_hands + j as usize];
                        }
                        true
                    }
                },
            ),
        })
    }
}

fn overlaps(board: u64, h: Hand) -> bool {
    board >> h.0 & 1 == 1 || board >> h.1 & 1 == 1
}

fn board_mask(game: &PostFlopGame) -> u64 {
    game.current_board().iter().fold(0, |m, &c| m | 1 << c)
}

fn export_walk(
    game: &mut PostFlopGame,
    history: &mut Vec<usize>,
    line: &mut Vec<LineStep>,
    profile: &mut StrategyProfile,
) -> Result<(), String> {
    game.apply_history(history);
    if game.is_terminal_node() {
        return Ok(());
    }
    let children: Vec<(usize, LineStep)> = if game.is_chance_node() {
        let cards = game.possible_cards();
        (0..52u8)
            .filter(|&c| cards >> c & 1 == 1)
            .map(|c| (c as usize, LineStep::Card(c)))
            .collect()
    } else {
        let player = game.current_player();
        let actions = game
            .available_actions()
            .into_iter()
            .map(|a| ActionKey::from_action(a).ok_or_else(|| format!("Unexpected action {a:?}")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut strategy = game.strategy();
        let (board, hands) = (board_mask(game), game.private_cards(player));
        for (j, &h) in hands.iter().enumerate() {
            if overlaps(board, h) {
                for a in 0..actions.len() {
                    strategy[a * hands.len() + j] = f32::NAN;
                }
            }
        }
        profile.push_node(line.clone(), player, actions.clone(), &strategy)?;
        actions
            .into_iter()
            .enumerate()
            .map(|(i, a)| (i, LineStep::Action(a)))
            .collect()
    };
    for (child, step) in children {
        history.push(child);
        line.push(step);
        export_walk(game, history, line, profile)?;
        line.pop();
        history.pop();
    }
    Ok(())
}

/// How source actions are matched to ours.
///
/// Fold/Check/Call match exactly. Bet/Raise/AllIn form one aggressive class: a source all-in
/// matches our all-in whatever its amount; otherwise a source action matches our closest amount
/// within `max(tol_chips, tol_rel * amount)` after scaling (none if two are equally close).
/// Several source actions may match one of ours; only one of them may have mass. Source mass on an
/// unmatched action makes the node mismatched.
#[derive(Debug, Clone, Copy)]
pub struct MatchOptions {
    /// Factor applied to source amounts; `None` = our pot / source pot (1.0 if unknown).
    pub amount_scale: Option<f64>,
    pub tol_chips: f64,
    pub tol_rel: f64,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            amount_scale: None,
            tol_chips: 1.0,
            tol_rel: 1e-3,
        }
    }
}

/// For each source action, the index of the closest matching action of ours.
fn match_actions(
    ours: &[Action],
    theirs: &[ActionKey],
    scale: f64,
    opts: &MatchOptions,
) -> Vec<Option<usize>> {
    let has_allin = ours.iter().any(|a| matches!(a, Action::AllIn(_)));
    let distance = |ours: Action, t: &ActionKey| match (ours, t) {
        (Action::Fold, ActionKey::Fold)
        | (Action::Check, ActionKey::Check)
        | (Action::Call, ActionKey::Call)
        | (Action::AllIn(_), ActionKey::AllIn(_)) => Some(0.0),
        (_, ActionKey::AllIn(_)) if has_allin => None,
        (Action::Bet(x) | Action::Raise(x) | Action::AllIn(x), _) => {
            let x = x as f64;
            t.amount()
                .map(|y| (x - y * scale).abs())
                .filter(|&d| d <= opts.tol_chips.max(opts.tol_rel * x))
        }
        _ => None,
    };
    theirs
        .iter()
        .map(|t| {
            let (mut best, mut tie) = (None, false);
            for (i, &a) in ours.iter().enumerate() {
                let Some(d) = distance(a, t) else { continue };
                match best {
                    Some((_, b)) if d > b => {}
                    Some((_, b)) if d == b => tie = true,
                    _ => (best, tie) = (Some((i, d)), false),
                }
            }
            best.filter(|_| !tie).map(|(i, _)| i)
        })
        .collect()
}

/// Index of the action in `available` matching `key` (see [`MatchOptions`]; `amount_scale`
/// defaults to 1.0), or `None` if none or two equally close match.
pub fn match_action(available: &[Action], key: ActionKey, opts: &MatchOptions) -> Option<usize> {
    match_actions(available, &[key], opts.amount_scale.unwrap_or(1.0), opts)[0]
}

fn unscale(key: ActionKey, scale: f64) -> ActionKey {
    match key {
        ActionKey::Bet(x) => ActionKey::Bet(x / scale),
        ActionKey::Raise(x) => ActionKey::Raise(x / scale),
        ActionKey::AllIn(x) => ActionKey::AllIn(x / scale),
        other => other,
    }
}

/// Which players [`lock_source`] locks.
#[derive(Debug, Clone, Copy)]
pub struct LockOptions {
    pub players: [bool; 2],
    pub matching: MatchOptions,
}

impl Default for LockOptions {
    fn default() -> Self {
        Self {
            players: [true, true],
            matching: MatchOptions::default(),
        }
    }
}

/// Locking statistics of one player (only counted for locked players).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlayerLockReport {
    /// Action nodes visited (isomorphic chances count once).
    pub action_nodes: u64,
    pub locked: u64,
    /// Nodes the source does not cover.
    pub missing: u64,
    /// Nodes of the other player in the source, or with source mass on an action we lack or on
    /// two source actions matching one of ours.
    pub mismatched: u64,
    /// Hands left free at locked nodes (NaN or non-positive row), board conflicts excluded.
    pub free_hands: u64,
    /// Hands the source does not know at locked nodes.
    pub unmapped_hands: u64,
}

/// Result of [`lock_source`].
#[derive(Debug, Clone, Default)]
pub struct LockReport {
    pub players: [PlayerLockReport; 2],
    /// First few problems, as `"<OOP|IP> <kind> <our line>"` (missing: top of each subtree).
    pub issues: Vec<String>,
}

impl LockReport {
    /// Whether every node and hand of the locked players came from the source.
    pub fn is_complete(&self) -> bool {
        self.players.iter().all(|p| {
            p.missing == 0 && p.mismatched == 0 && p.free_hands == 0 && p.unmapped_hands == 0
        })
    }

    fn issue(&mut self, player: usize, kind: &str, line: &[LineStep]) {
        if self.issues.len() < MAX_ISSUES {
            let who = ["OOP", "IP"][player];
            self.issues
                .push(format!("{who} {kind} {:?}", line_key(line)));
        }
    }
}

/// How the source line to the current node was obtained.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Cover {
    /// From the source's own action keys.
    Source,
    /// Below a node the source lacks: our actions with unscaled amounts (best effort).
    Guessed,
    /// Below an action of ours that no source action matches: not looked up.
    Lost,
}

struct LockCtx<'a> {
    source: &'a dyn StrategySource,
    opts: &'a LockOptions,
    scale: f64,
    report: LockReport,
    /// Our own line to the current node, for issues.
    our_line: Vec<LineStep>,
}

fn check_lockable(game: &PostFlopGame) -> Result<(), String> {
    if game.is_ready() {
        Ok(())
    } else {
        Err("Game must have memory allocated (full storage) and not be solved".into())
    }
}

/// Locks the nodes of the selected players to `source` and unlocks every other node.
///
/// Walks our tree (representative chance cards only, since isomorphic cards alias one node),
/// so the game must be allocated and not finalized. Nodes that are missing or mismatched stay
/// free. Leaves the game at the root; on error nothing is changed.
pub fn lock_source(
    game: &mut PostFlopGame,
    source: &dyn StrategySource,
    opts: LockOptions,
) -> Result<LockReport, String> {
    check_lockable(game)?;
    let our_pot = game.tree_config().starting_pot as f64;
    let scale = opts
        .matching
        .amount_scale
        .or_else(|| source.starting_pot().map(|p| our_pot / p))
        .unwrap_or(1.0);
    if !(scale.is_finite() && scale > 0.0) {
        return Err(format!("Invalid amount scale: {scale}"));
    }
    let mut ctx = LockCtx {
        source,
        opts: &opts,
        scale,
        report: LockReport::default(),
        our_line: Vec::new(),
    };
    lock_walk(
        game,
        &mut Vec::new(),
        &mut Vec::new(),
        Cover::Source,
        &mut ctx,
    );
    game.back_to_root();
    Ok(ctx.report)
}

/// `line` is the source's line to the current node.
fn lock_walk(
    game: &mut PostFlopGame,
    history: &mut Vec<usize>,
    line: &mut Vec<LineStep>,
    cover: Cover,
    ctx: &mut LockCtx,
) {
    game.apply_history(history);
    if game.is_terminal_node() {
        return;
    }

    if game.is_chance_node() {
        for action in game.available_actions() {
            if let Action::Chance(card) = action {
                history.push(card as usize);
                line.push(LineStep::Card(card));
                ctx.our_line.push(LineStep::Card(card));
                lock_walk(game, history, line, cover, ctx);
                ctx.our_line.pop();
                line.pop();
                history.pop();
            }
        }
        return;
    }

    let player = game.current_player();
    let actions = game.available_actions();
    let our_keys: Vec<ActionKey> = actions
        .iter()
        .filter_map(|&a| ActionKey::from_action(a))
        .collect();
    let selected = ctx.opts.players[player];
    let report = &mut ctx.report;
    if selected {
        report.players[player].action_nodes += 1;
    }

    // source line step below each of our actions (`None`: lost), and their cover
    let (child_steps, child_cover, lock): (Vec<Option<LineStep>>, _, _) = match (cover
        != Cover::Lost)
        .then(|| ctx.source.node(line))
        .flatten()
    {
        None => {
            if selected {
                report.players[player].missing += 1;
                if cover == Cover::Source {
                    report.issue(player, "missing", &ctx.our_line);
                }
            }
            let steps = our_keys
                .iter()
                .map(|&k| Some(LineStep::Action(unscale(k, ctx.scale))))
                .collect();
            (steps, Cover::Guessed, None)
        }
        Some(node) => {
            let mapping = match_actions(&actions, &node.actions, ctx.scale, &ctx.opts.matching);
            let same_player = node.player == player;
            let collision = (0..mapping.len())
                .any(|s| mapping[s].is_some() && mapping[..s].contains(&mapping[s]));

            // read the hand rows when locking, or to resolve collisions by mass
            let hands = game.private_cards(player);
            let (board, num_hands) = (board_mask(game), hands.len());
            let mut strategy = vec![0.0; actions.len() * num_hands];
            let mut mass = vec![false; node.actions.len()];
            let (mut free, mut unmapped) = (0, 0);
            if same_player && (selected || collision) {
                let mut row = vec![0.0; node.actions.len()];
                for (j, &h) in hands.iter().enumerate() {
                    if overlaps(board, h) {
                        continue;
                    }
                    row.fill(f32::NAN);
                    if !(node.dist)(h, &mut row) {
                        unmapped += 1;
                        continue;
                    }
                    if row.iter().any(|v| v.is_nan()) || !row.iter().any(|&v| v > 0.0) {
                        free += 1;
                        continue;
                    }
                    for (s, &v) in row.iter().enumerate() {
                        mass[s] |= v > 0.0;
                        if let Some(a) = mapping[s] {
                            strategy[a * num_hands + j] += v.max(0.0);
                        }
                    }
                }
            }

            // below each of our actions follow its source action with mass (or the first one)
            let mut clash = false;
            let steps = (0..actions.len())
                .map(|a| {
                    let sources: Vec<usize> = (0..mapping.len())
                        .filter(|&s| mapping[s] == Some(a))
                        .collect();
                    let s = match sources.iter().filter(|&&s| mass[s]).count() {
                        0 => sources.first().copied(),
                        1 => sources.into_iter().find(|&s| mass[s]),
                        _ => {
                            clash = true;
                            None
                        }
                    };
                    s.map(|s| LineStep::Action(node.actions[s]))
                })
                .collect();
            let stray = (0..mapping.len()).any(|s| mapping[s].is_none() && mass[s]);

            let mut lock = None;
            if selected {
                let stats = &mut report.players[player];
                if !same_player {
                    stats.mismatched += 1;
                    report.issue(player, "player mismatch", &ctx.our_line);
                } else if stray || clash {
                    stats.mismatched += 1;
                    report.issue(player, "mismatched", &ctx.our_line);
                } else {
                    stats.locked += 1;
                    stats.free_hands += free;
                    stats.unmapped_hands += unmapped;
                    lock = Some(strategy);
                }
            }
            (steps, Cover::Source, lock)
        }
    };

    match lock {
        Some(strategy) => game.lock_current_strategy(&strategy),
        None => game.unlock_current_strategy(),
    }

    let children = child_steps.into_iter().zip(our_keys);
    for (i, (step, key)) in children.enumerate() {
        history.push(i);
        ctx.our_line.push(LineStep::Action(key));
        match step {
            Some(step) => {
                line.push(step);
                lock_walk(game, history, line, child_cover, ctx);
                line.pop();
            }
            None => lock_walk(game, history, line, Cover::Lost, ctx),
        }
        ctx.our_line.pop();
        history.pop();
    }
}

/// Options of [`evaluate_source`].
#[derive(Debug, Clone, Copy, Default)]
pub struct EvaluateOptions {
    pub matching: MatchOptions,
    /// CFR iterations run with both players locked before measuring, so the parts the source does
    /// not cover follow a solved strategy instead of the uniform one.
    pub fill_iterations: u32,
}

/// Result of [`evaluate_source`]. EVs exclude the `starting_pot / 2` bias, as in
/// [`compute_current_ev`].
#[derive(Debug, Clone)]
pub struct ProfileEvaluation {
    /// EV of each player's best response against the other's profile.
    pub best_response_ev: [f32; 2],
    /// EV of the profile against itself.
    pub profile_ev: [f32; 2],
    /// `((br0 - pev0) + (br1 - pev1)) / 2` (also valid with rake/ICM).
    pub exploitability: f32,
    /// Exploitability in % of [`PostFlopGame::exploitability_target_scale`] (the pot in chipEV).
    pub exploitability_pct: f32,
    /// [`LockReport::is_complete`] of the final pass.
    pub complete: bool,
    /// Report of the final pass (both players locked).
    pub report: LockReport,
}

/// Measures `source` with our best response, without CFR.
///
/// Locks also bind the best response of the locked player, so each best response is computed
/// with only the opponent locked; the profile EV with both locked. Leaves the game allocated
/// with both players locked: the caller may [`finalize`] it and read per-hand EVs.
pub fn evaluate_source(
    game: &mut PostFlopGame,
    source: &dyn StrategySource,
    opts: EvaluateOptions,
) -> Result<ProfileEvaluation, String> {
    let lock = |players| LockOptions {
        players,
        matching: opts.matching,
    };
    if opts.fill_iterations > 0 {
        lock_source(game, source, lock([true, true]))?;
        for i in 0..opts.fill_iterations {
            solve_step(game, i);
        }
    }
    let mut best_response_ev = [0.0; 2];
    for (locked, free) in [(0, 1), (1, 0)] {
        let mut players = [false; 2];
        players[locked] = true;
        lock_source(game, source, lock(players))?;
        best_response_ev[free] = compute_mes_ev(game)[free];
    }
    let report = lock_source(game, source, lock([true, true]))?;
    let profile_ev = compute_current_ev(game);
    let exploitability =
        ((best_response_ev[0] - profile_ev[0]) + (best_response_ev[1] - profile_ev[1])) * 0.5;
    Ok(ProfileEvaluation {
        best_response_ev,
        profile_ev,
        exploitability,
        exploitability_pct: 100.0 * exploitability / game.exploitability_target_scale(),
        complete: report.is_complete(),
        report,
    })
}

/// Converts a typed line into a history for [`PostFlopGame::apply_history`], matching actions
/// with `opts` (`amount_scale` defaults to 1.0). The game must be allocated with full storage.
/// Leaves the game at the resolved node.
pub fn resolve_line(
    game: &mut PostFlopGame,
    line: &[LineStep],
    opts: &MatchOptions,
) -> Result<Vec<usize>, String> {
    if !game.has_full_storage() {
        return Err("Game must have memory allocated with full storage".into());
    }
    game.back_to_root();
    let mut history = Vec::with_capacity(line.len());
    for (k, step) in line.iter().enumerate() {
        let at = || format!("{:?}", line_key(&line[..=k]));
        if game.is_terminal_node() {
            return Err(format!("Line continues past a terminal node: {}", at()));
        }
        let index = match *step {
            LineStep::Card(card) if game.is_chance_node() => {
                if card >= 52 || game.possible_cards() >> card & 1 == 0 {
                    return Err(format!("Card cannot be dealt: {}", at()));
                }
                card as usize
            }
            LineStep::Action(key) if !game.is_chance_node() => {
                match match_action(&game.available_actions(), key, opts) {
                    Some(i) => i,
                    None => return Err(format!("No unique matching action: {}", at())),
                }
            }
            _ => return Err(format!("Step does not fit the node kind: {}", at())),
        };
        game.play(index);
        history.push(index);
    }
    Ok(history)
}

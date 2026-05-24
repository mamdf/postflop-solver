mod base;
mod evaluation;
mod interpreter;
mod node;

#[cfg(feature = "bincode")]
mod serialization;

#[cfg(test)]
mod tests;

use crate::action_tree::*;
use crate::card::*;
use crate::mutex_like::*;
use std::collections::BTreeMap;

#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[cfg_attr(feature = "bincode", derive(Decode, Encode))]
enum State {
    ConfigError = 0,
    #[default]
    Uninitialized = 1,
    TreeBuilt = 2,
    MemoryAllocated = 3,
    Solved = 4,
}

/// A struct representing a postflop game.
#[derive(Default)]
pub struct PostFlopGame {
    // state
    state: State,

    // postflop game configurations
    card_config: CardConfig,
    tree_config: TreeConfig,
    added_lines: Vec<Vec<Action>>,
    removed_lines: Vec<Vec<Action>>,
    action_root: Box<MutexLike<ActionTreeNode>>,

    // computed from configurations
    num_combinations: f64,
    initial_weights: [Vec<f32>; 2],
    private_cards: [Vec<(Card, Card)>; 2],
    same_hand_index: [Vec<u16>; 2],

    // indices in `private_cards` that do not conflict with the specified board cards
    valid_indices_flop: [Vec<u16>; 2],
    valid_indices_turn: Vec<[Vec<u16>; 2]>,
    valid_indices_river: Vec<[Vec<u16>; 2]>,

    // hand strength information: indices are stored in ascending strength order
    hand_strength: Vec<[Vec<StrengthItem>; 2]>,

    // isomorphism information
    // - `isomorphism_ref_*`: indices to which the eliminated events should refer
    // - `isomorphism_card_*`: list of cards eliminated by the isomorphism
    // - `isomorphism_swap_*`: list of hand index pairs that should be swapped when applying the
    //                         isomorphism with the specified suit
    isomorphism_ref_turn: Vec<u8>,
    isomorphism_card_turn: Vec<Card>,
    isomorphism_swap_turn: [SwapList; 4],
    isomorphism_ref_river: Vec<Vec<u8>>,
    isomorphism_card_river: [Vec<Card>; 4],
    isomorphism_swap_river: [[SwapList; 4]; 4],

    // bunching effect
    bunching_num_dead_cards: usize,
    bunching_num_combinations: f64,
    bunching_arena: Vec<f32>,
    bunching_strength: Vec<[Vec<u16>; 2]>,
    bunching_num_flop: [Vec<usize>; 2],
    bunching_num_turn: [Vec<Vec<usize>>; 2],
    bunching_num_river: [Vec<Vec<usize>>; 2],
    bunching_coef_flop: [Vec<usize>; 2],
    bunching_coef_turn: [Vec<Vec<usize>>; 2],

    // store options
    storage_mode: BoardState,
    target_storage_mode: BoardState,
    num_nodes: [u64; 3],
    is_compression_enabled: bool,
    num_storage: u64,
    num_storage_ip: u64,
    num_storage_chance: u64,
    misc_memory_usage: u64,

    // global storage
    // `storage*` are used as a global storage and are referenced by `PostFlopNode::storage*`.
    // Methods like `PostFlopNode::strategy` define how the storage is used.
    node_arena: Vec<MutexLike<PostFlopNode>>,
    storage1: Vec<u8>,
    storage2: Vec<u8>,
    storage_ip: Vec<u8>,
    storage_chance: Vec<u8>,
    locking_strategy: BTreeMap<usize, Vec<f32>>,

    // result interpreter
    action_history: Vec<usize>,
    node_history: Vec<usize>,
    is_normalized_weight_cached: bool,
    turn: Card,
    river: Card,
    turn_swapped_suit: Option<(u8, u8)>,
    turn_swap: Option<u8>,
    river_swap: Option<(u8, u8)>,
    total_bet_amount: [i32; 2],
    weights: [Vec<f32>; 2],
    normalized_weights: [Vec<f32>; 2],
    cfvalues_cache: [Vec<f32>; 2],

    // Tournament ICM is runtime-only in C1 and is not persisted by custom serialization yet.
    tournament_icm_config: Option<TournamentIcmConfig>,
    terminal_icm_utilities: Vec<Option<TerminalIcmUtility>>,
}

/// Public unit used by additive EV reporting APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "bincode", derive(Decode, Encode))]
pub enum ExpectedValueUnit {
    Chips,
    PayoutDelta,
}

/// Raw storage element counts that back [`PostFlopGame::memory_usage`].
///
/// These counts proxy the action-tree size and are stable across compression
/// settings, making them suitable as features for runtime predictors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StorageCounts {
    /// Storage elements for OOP-perspective action regrets/strategy at action nodes.
    pub num_storage: u64,
    /// Storage elements for IP-perspective regrets/strategy at action nodes.
    pub num_storage_ip: u64,
    /// Storage elements for chance (turn/river card) transitions.
    pub num_storage_chance: u64,
}

impl StorageCounts {
    /// Total elements: `2 * num_storage + num_storage_ip + num_storage_chance`,
    /// matching the formula used inside [`PostFlopGame::memory_usage`].
    #[inline]
    pub fn total_elements(&self) -> u64 {
        2 * self.num_storage + self.num_storage_ip + self.num_storage_chance
    }
}

/// Runtime tournament ICM configuration for exact terminal utility precomputation.
#[derive(Debug, Clone, PartialEq)]
pub struct TournamentIcmConfig {
    pub stacks: Vec<f64>,
    pub payouts: Vec<i32>,
    pub oop_seat: usize,
    pub ip_seat: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TerminalIcmUtility {
    pub(crate) win: [f32; 2],
    pub(crate) lose: [f32; 2],
    pub(crate) tie: [f32; 2],
}

/// A struct representing a node in a postflop game tree.
///
/// The nodes must be stored as `Vec<MutexLike<PostFlopNode>>`.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PostFlopNode {
    prev_action: Action,
    player: u8,
    turn: Card,
    river: Card,
    is_locked: bool,
    amount: i32,
    children_offset: u32,
    num_children: u16,
    num_elements_ip: u16,
    num_elements: u32,
    scale1: f32,
    scale2: f32,
    scale3: f32,
    storage1: *mut u8, // strategy
    storage2: *mut u8, // regrets or cfvalues
    storage3: *mut u8, // IP cfvalues
}

unsafe impl Send for PostFlopNode {}
unsafe impl Sync for PostFlopNode {}

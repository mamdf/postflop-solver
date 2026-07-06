use super::*;
use crate::bunching::*;
use crate::icm::{normalize_payouts, terminal_icm_values};
use crate::interface::*;
use crate::utility::*;
use std::collections::BTreeMap;
use std::mem::{self, MaybeUninit};

#[cfg(feature = "rayon")]
use rayon::prelude::*;

#[derive(Default)]
struct BuildTreeInfo {
    flop_index: usize,
    turn_index: usize,
    river_index: usize,
    num_storage: u64,
    num_storage_ip: u64,
    num_storage_chance: u64,
}

impl Game for PostFlopGame {
    type Node = PostFlopNode;

    #[inline]
    fn root(&self) -> MutexGuardLike<Self::Node> {
        self.node_arena[0].lock()
    }

    #[inline]
    fn num_private_hands(&self, player: usize) -> usize {
        self.private_cards[player].len()
    }

    #[inline]
    fn initial_weights(&self, player: usize) -> &[f32] {
        &self.initial_weights[player]
    }

    #[inline]
    fn evaluate(
        &self,
        result: &mut [MaybeUninit<f32>],
        node: &Self::Node,
        player: usize,
        cfreach: &[f32],
    ) {
        if self.bunching_num_dead_cards == 0 {
            self.evaluate_internal(result, node, player, cfreach);
        } else {
            self.evaluate_internal_bunching(result, node, player, cfreach);
        }
    }

    #[inline]
    fn chance_factor(&self, node: &Self::Node) -> usize {
        if node.turn == NOT_DEALT {
            45 - self.bunching_num_dead_cards
        } else {
            44 - self.bunching_num_dead_cards
        }
    }

    #[inline]
    fn is_solved(&self) -> bool {
        self.state == State::Solved
    }

    #[inline]
    fn set_solved(&mut self) {
        self.state = State::Solved;
        let history = self.action_history.clone();
        self.apply_history(&history);
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.state == State::MemoryAllocated && self.storage_mode == BoardState::River
    }

    #[inline]
    fn is_raked(&self) -> bool {
        self.tree_config.rake_rate > 0.0 && self.tree_config.rake_cap > 0.0
    }

    #[inline]
    fn uses_bubble_factor(&self) -> bool {
        self.tree_config.bubble_factor != [1.0, 1.0]
    }

    #[inline]
    fn uses_custom_terminal_utility(&self) -> bool {
        self.uses_bubble_factor() || self.uses_tournament_icm()
    }

    #[inline]
    fn isomorphic_chances(&self, node: &Self::Node) -> &[u8] {
        if node.turn == NOT_DEALT {
            &self.isomorphism_ref_turn
        } else {
            &self.isomorphism_ref_river[node.turn as usize]
        }
    }

    #[inline]
    fn isomorphic_swap(&self, node: &Self::Node, index: usize) -> &[Vec<(u16, u16)>; 2] {
        if node.turn == NOT_DEALT {
            &self.isomorphism_swap_turn[self.isomorphism_card_turn[index] as usize & 3]
        } else {
            &self.isomorphism_swap_river[node.turn as usize & 3]
                [self.isomorphism_card_river[node.turn as usize & 3][index] as usize & 3]
        }
    }

    #[inline]
    fn locking_strategy(&self, node: &Self::Node) -> &[f32] {
        if !node.is_locked {
            &[]
        } else {
            let index = self.node_index(node);
            self.locking_strategy.get(&index).unwrap()
        }
    }

    #[inline]
    fn is_compression_enabled(&self) -> bool {
        self.is_compression_enabled
    }
}

impl PostFlopGame {
    /// Creates a new empty [`PostFlopGame`].
    ///
    /// Use of this method is strongly discouraged because an instance created by this method is
    /// invalid until [`update_config`] is called.
    /// Please use [`with_config`] instead whenever possible.
    ///
    /// [`update_config`]: #method.update_config
    /// [`with_config`]: #method.with_config
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`PostFlopGame`] with the specified configuration.
    #[inline]
    pub fn with_config(card_config: CardConfig, action_tree: ActionTree) -> Result<Self, String> {
        let mut game = Self::new();
        game.update_config(card_config, action_tree)?;
        Ok(game)
    }

    /// Updates the game configuration. The solved result will be lost.
    #[inline]
    pub fn update_config(
        &mut self,
        card_config: CardConfig,
        action_tree: ActionTree,
    ) -> Result<(), String> {
        self.state = State::ConfigError;

        if !action_tree.invalid_terminals().is_empty() {
            return Err("Invalid terminal is found in action tree".to_string());
        }

        self.card_config = card_config;
        (
            self.tree_config,
            self.added_lines,
            self.removed_lines,
            self.action_root,
        ) = action_tree.eject();

        self.check_card_config()?;
        self.init_card_fields();
        self.init_root()?;

        self.state = State::TreeBuilt;

        self.init_interpreter();
        self.reset_bunching_effect();
        self.clear_tournament_icm_internal();

        Ok(())
    }

    /// Sets the bunching effect configuration.
    ///
    /// **Warning**: Enabling the bunching effect will significantly slow down the solving process.
    /// Specifically, the computational complexity of the terminal evaluation will increase from
    /// *O*(#(OOP private hands) + #(IP private hands)) to *O*(#(OOP private hands) * #(IP private
    /// hands)).
    #[inline]
    pub fn set_bunching_effect(&mut self, bunching_data: &BunchingData) -> Result<(), String> {
        if self.state <= State::Uninitialized {
            return Err("Game is not successfully initialized".to_string());
        }

        if !bunching_data.is_ready() {
            return Err("Bunching configuration is not ready".to_string());
        }

        let mut flop_sorted = self.card_config.flop;
        flop_sorted.sort_unstable();
        if flop_sorted != bunching_data.flop() {
            return Err("Flop cards do not match".to_string());
        }

        self.reset_bunching_effect();
        self.set_bunching_effect_internal(bunching_data)?;

        Ok(())
    }

    /// Resets the bunching effect configuration. The current node will also be reset to the root.
    #[inline]
    pub fn reset_bunching_effect(&mut self) {
        self.bunching_num_dead_cards = 0;
        self.bunching_num_combinations = 0.0;
        self.bunching_arena = Vec::new();
        self.bunching_strength = Vec::new();
        self.bunching_num_flop = Default::default();
        self.bunching_num_turn = Default::default();
        self.bunching_num_river = Default::default();
        self.bunching_coef_flop = Default::default();
        self.bunching_coef_turn = Default::default();
        self.back_to_root();
    }

    /// Obtains the card configuration.
    #[inline]
    pub fn card_config(&self) -> &CardConfig {
        &self.card_config
    }

    /// Obtains the tree configuration.
    #[inline]
    pub fn tree_config(&self) -> &TreeConfig {
        &self.tree_config
    }

    /// Returns whether bubble-factor payoff scaling is enabled.
    #[inline]
    pub fn uses_bubble_factor(&self) -> bool {
        self.tree_config.bubble_factor != [1.0, 1.0]
    }

    /// Configures exact tournament ICM terminal utility precomputation.
    ///
    /// The small config (and the per-seat base contribution) is persisted by serialization; the
    /// large precomputed terminal utilities are re-derived on load. Strategy uses the ICM deltas
    /// internally, while public displayed EV conversion is deferred to a later phase.
    /// When enabled, solver utilities are payout-unit deltas, not chipEV pot units. Callers should
    /// build `target_exploitability` via
    /// [`target_exploitability_from_fraction`](Self::target_exploitability_from_fraction), which
    /// yields pot-comparable targets in both modes, or use `0.0` to force iterations when testing
    /// ICM behavior.
    pub fn set_tournament_icm_config(
        &mut self,
        config: TournamentIcmConfig,
    ) -> Result<usize, String> {
        self.set_tournament_icm_config_inner(config, [0, 0])
    }

    /// Same as [`set_tournament_icm_config`](Self::set_tournament_icm_config), but threads a
    /// per-seat `base_contribution`: the chips each seat invested BEFORE this game began.
    ///
    /// This is the entry point for re-rooted river games. A re-rooted river game has a
    /// `tree_config.starting_pot` that already folds in the pre-river pot, so its in-tree
    /// `contribution` only measures river-local investment. Passing the pre-river investment as
    /// `base_contribution` lets the terminal ICM utilities deduct the FULL hand investment from
    /// each seat's absolute tournament stack, reconstructing the true full-hand outcome.
    ///
    /// `base_contribution` is consumed at config time and baked into the precomputed
    /// `terminal_icm_utilities`. It is not stored on the config, but it IS persisted by
    /// serialization (format `2026-07-06`), so re-rooted subgames round-trip correctly;
    /// files written by the previous format load with `base == [0, 0]`.
    pub fn set_tournament_icm_config_with_base_contribution(
        &mut self,
        config: TournamentIcmConfig,
        base_contribution: [i32; 2],
    ) -> Result<usize, String> {
        self.set_tournament_icm_config_inner(config, base_contribution)
    }

    fn set_tournament_icm_config_inner(
        &mut self,
        mut config: TournamentIcmConfig,
        base_contribution: [i32; 2],
    ) -> Result<usize, String> {
        if self.state != State::TreeBuilt {
            return Err("Tournament ICM can only be configured after tree build and before memory allocation".to_string());
        }

        if self.tree_config.rake_rate > 0.0 && self.tree_config.rake_cap > 0.0 {
            return Err("Tournament ICM cannot be combined with rake in this phase".to_string());
        }

        if self.tree_config.bubble_factor != [1.0, 1.0] {
            return Err(
                "Tournament ICM cannot be combined with bubble factor in this phase".to_string(),
            );
        }

        let player_count = config.stacks.len();
        if player_count == 0 {
            return Err("Tournament ICM stacks must not be empty".to_string());
        }

        if config.oop_seat >= player_count || config.ip_seat >= player_count {
            return Err("Tournament ICM OOP/IP seats must be inside stacks".to_string());
        }

        if config.oop_seat == config.ip_seat {
            return Err("Tournament ICM OOP/IP seats must be different".to_string());
        }

        if config
            .stacks
            .iter()
            .any(|&stack| !stack.is_finite() || stack < 0.0)
        {
            return Err("Tournament ICM stacks must be finite and non-negative".to_string());
        }

        config.payouts = normalize_payouts(&config.payouts, player_count);

        let baseline_values =
            self.tournament_icm_values_for_terminal(&config, [0, 0], base_contribution, None)?;
        let baseline_oop = baseline_values[config.oop_seat];
        let baseline_ip = baseline_values[config.ip_seat];

        let utilities = self.precompute_terminal_icm_utilities(
            &config,
            base_contribution,
            baseline_oop,
            baseline_ip,
        )?;

        self.tournament_icm_config = Some(config);
        self.terminal_icm_utilities = utilities;
        // Retain the base for the runtime fold-equivalent path; the terminal utilities already
        // baked it in, but `tournament_icm_fold_equivalent_delta` recomputes its baseline/fold
        // stacks live and must use the SAME base to stay consistent.
        self.base_contribution = base_contribution;

        Ok(self.tournament_icm_utility_count())
    }

    /// Clears the runtime tournament ICM configuration.
    pub fn clear_tournament_icm_config(&mut self) -> Result<(), String> {
        if self.state != State::TreeBuilt {
            return Err("Tournament ICM can only be cleared before memory allocation".to_string());
        }

        self.clear_tournament_icm_internal();
        Ok(())
    }

    /// Re-derives the tournament ICM terminal utilities from the persisted config after
    /// deserialization.
    ///
    /// The precompute is pure — it depends only on the node arena, the tree config, the ICM
    /// config, and the base contribution — so it reconstructs the exact utilities that custom
    /// serialization drops from the file. The per-seat `base_contribution` comes from the
    /// decoded stream (legacy-format files decode it to `[0, 0]`, preserving their historical
    /// behavior). Unlike [`set_tournament_icm_config`](Self::set_tournament_icm_config), it
    /// bypasses the `TreeBuilt` state gate because a loaded game is already solved, and it
    /// is a no-op when no ICM config was persisted.
    #[cfg(feature = "bincode")]
    pub(super) fn restore_tournament_icm_from_config(&mut self) -> Result<(), String> {
        let Some(config) = self.tournament_icm_config.take() else {
            return Ok(());
        };
        let base_contribution = self.base_contribution;
        let baseline_values =
            self.tournament_icm_values_for_terminal(&config, [0, 0], base_contribution, None)?;
        let baseline_oop = baseline_values[config.oop_seat];
        let baseline_ip = baseline_values[config.ip_seat];
        self.terminal_icm_utilities = self.precompute_terminal_icm_utilities(
            &config,
            base_contribution,
            baseline_oop,
            baseline_ip,
        )?;
        self.tournament_icm_config = Some(config);
        Ok(())
    }

    /// Returns whether exact tournament ICM terminal utilities are enabled.
    #[inline]
    pub fn uses_tournament_icm(&self) -> bool {
        self.tournament_icm_config.is_some()
    }

    /// Returns the number of terminal nodes with precomputed ICM utilities.
    #[inline]
    pub fn tournament_icm_utility_count(&self) -> usize {
        self.terminal_icm_utilities
            .iter()
            .filter(|utility| utility.is_some())
            .count()
    }

    /// Returns a utility scale suitable for converting a relative exploitability target into the
    /// raw utility units expected by [`solve`](crate::solve).
    ///
    /// For chipEV and bubble-factor modes this is the starting pot in chips. For tournament ICM,
    /// this is the ICM payout-unit value of the starting pot: the mean over both players of their
    /// ICM value swing between winning and losing the bare pot at zero in-tree contribution. The
    /// same fraction therefore means "fraction of the starting pot's worth" in both modes. A
    /// caller that wants a `0.5%` target can use `game.exploitability_target_scale() * 0.005`
    /// after configuring tournament ICM and before solving.
    ///
    /// If both seats sit on a flat segment of the payout structure, the pot swing is `0.0`, so any
    /// relative target derived from it is `0.0` and the solve runs to its iteration limit.
    #[inline]
    pub fn exploitability_target_scale(&self) -> f32 {
        if self.uses_tournament_icm() {
            self.tournament_icm_pot_swing().unwrap_or_else(|| {
                // Unreachable for trees built through the public API: ICM config setup already
                // validated the zero-contribution terminal with the same computation. Fall back
                // to the (much looser) max terminal delta rather than panicking in release.
                debug_assert!(false, "tournament ICM pot swing unavailable");
                self.tournament_icm_max_terminal_delta()
            })
        } else {
            self.tree_config.starting_pot as f32
        }
    }

    /// Computes the mean over both players of their ICM value swing between winning and losing
    /// the bare starting pot at zero in-tree contribution, in payout units.
    fn tournament_icm_pot_swing(&self) -> Option<f32> {
        let config = self.tournament_icm_config.as_ref()?;
        let win0 = self
            .tournament_icm_values_for_terminal(config, [0, 0], self.base_contribution, Some(0))
            .ok()?;
        let win1 = self
            .tournament_icm_values_for_terminal(config, [0, 0], self.base_contribution, Some(1))
            .ok()?;
        let oop_swing = win0[config.oop_seat] - win1[config.oop_seat];
        let ip_swing = win1[config.ip_seat] - win0[config.ip_seat];
        Some(((oop_swing + ip_swing) * 0.5) as f32)
    }

    /// Fallback scale: the largest absolute precomputed terminal ICM delta in payout units.
    fn tournament_icm_max_terminal_delta(&self) -> f32 {
        self.terminal_icm_utilities
            .iter()
            .filter_map(|&utility| utility)
            .flat_map(|utility| {
                utility
                    .win
                    .into_iter()
                    .chain(utility.lose)
                    .chain(utility.tie)
            })
            .map(f32::abs)
            .fold(0.0, f32::max)
    }

    /// Converts a relative exploitability target into raw utility units.
    ///
    /// `target_fraction` is a ratio, so `0.005` means `0.5%` of
    /// [`exploitability_target_scale`](Self::exploitability_target_scale).
    #[inline]
    pub fn target_exploitability_from_fraction(&self, target_fraction: f32) -> f32 {
        self.exploitability_target_scale() * target_fraction
    }

    #[inline]
    pub(crate) fn terminal_icm_utility_for_node(
        &self,
        node: &PostFlopNode,
    ) -> Option<TerminalIcmUtility> {
        let index = self.node_index(node);
        self.terminal_icm_utilities
            .get(index)
            .and_then(|&utility| utility)
    }

    pub(crate) fn tournament_icm_fold_equivalent_delta(
        &self,
        player: usize,
        contribution: [i32; 2],
    ) -> Option<f32> {
        let config = self.tournament_icm_config.as_ref()?;
        // Both calls MUST use the stored base so the fold-equivalent centering is consistent with
        // the offset baked into the terminal utilities. For re-rooted river games (base != [0, 0])
        // ICM equity is nonlinear in the stack vector, so passing [0, 0] here would mix an
        // un-offset centering with offset terminals and produce wrong reported EVs.
        let base = self.base_contribution;
        let baseline = self
            .tournament_icm_values_for_terminal(config, [0, 0], base, None)
            .ok()?;
        let fold = self
            .tournament_icm_values_for_terminal(config, contribution, base, Some(player ^ 1))
            .ok()?;
        let seat = if player == 0 {
            config.oop_seat
        } else {
            config.ip_seat
        };

        Some((fold[seat] - baseline[seat]) as f32)
    }

    /// Obtains the added lines.
    #[inline]
    pub fn added_lines(&self) -> &[Vec<Action>] {
        &self.added_lines
    }

    /// Obtains the removed lines.
    #[inline]
    pub fn removed_lines(&self) -> &[Vec<Action>] {
        &self.removed_lines
    }

    /// Returns the card list of private hands of the given player.
    ///
    /// The returned list contains only card pairs with positive weight, i.e., card pairs with zero
    /// weight are excluded. The returned list is sorted as follows:
    ///
    /// - Each card pair has IDs in `(low_id, high_id)` order.
    /// - Card pairs are sorted in the lexicographic order.
    #[inline]
    pub fn private_cards(&self, player: usize) -> &[(Card, Card)] {
        if self.state <= State::Uninitialized {
            panic!("Game is not successfully initialized");
        }

        &self.private_cards[player]
    }

    /// Returns the estimated memory usage in bytes (uncompressed, compressed).
    #[inline]
    pub fn memory_usage(&self) -> (u64, u64) {
        if self.state <= State::Uninitialized {
            panic!("Game is not successfully initialized");
        }

        let num_elements = 2 * self.num_storage + self.num_storage_ip + self.num_storage_chance;
        let uncompressed = 4 * num_elements + self.misc_memory_usage;
        let compressed = 2 * num_elements + self.misc_memory_usage;

        (uncompressed, compressed)
    }

    /// Returns the raw storage element counts that back [`Self::memory_usage`].
    ///
    /// Intended for runtime predictors and capacity planning: these counts
    /// proxy the action-tree size and do not depend on `compression`.
    #[inline]
    pub fn storage_counts(&self) -> StorageCounts {
        if self.state <= State::Uninitialized {
            panic!("Game is not successfully initialized");
        }
        StorageCounts {
            num_storage: self.num_storage,
            num_storage_ip: self.num_storage_ip,
            num_storage_chance: self.num_storage_chance,
        }
    }

    /// Returns the estimated additional memory usage in bytes when the bunching effect is enabled.
    #[inline]
    pub fn memory_usage_bunching(&self) -> u64 {
        if self.state <= State::Uninitialized {
            panic!("Game is not successfully initialized");
        }

        self.memory_usage_bunching_internal()
    }

    /// Remove lines after building the `PostFlopGame` but before allocating memory.
    ///
    /// This allows the removal of chance-specific lines (e.g., remove overbets on board-pairing
    /// turns) which we cannot do while building an action tree.
    pub fn remove_lines(&mut self, lines: &[Vec<Action>]) -> Result<(), String> {
        if self.state <= State::Uninitialized {
            return Err("Game is not successfully initialized".to_string());
        } else if self.state >= State::MemoryAllocated {
            return Err("Game has already been allocated".to_string());
        }

        for line in lines {
            let mut root = self.root();
            let info = self.remove_line_recursive(&mut root, line)?;
            self.num_storage -= info.num_storage;
            self.num_storage_ip -= info.num_storage_ip;
            self.num_storage_chance -= info.num_storage_chance;
        }

        Ok(())
    }

    /// Returns whether the memory is allocated.
    ///
    /// If the memory is allocated, returns `Some(is_compression_enabled)`;
    /// otherwise, returns `None`.
    #[inline]
    pub fn is_memory_allocated(&self) -> Option<bool> {
        if self.state <= State::TreeBuilt {
            None
        } else {
            Some(self.is_compression_enabled)
        }
    }

    /// Allocates the memory.
    pub fn allocate_memory(&mut self, enable_compression: bool) {
        if self.state <= State::Uninitialized {
            panic!("Game is not successfully initialized");
        }

        if self.state == State::MemoryAllocated
            && self.storage_mode == BoardState::River
            && self.is_compression_enabled == enable_compression
        {
            return;
        }

        let num_bytes = if enable_compression { 2 } else { 4 };
        if num_bytes * self.num_storage > isize::MAX as u64
            || num_bytes * self.num_storage_chance > isize::MAX as u64
        {
            panic!("Memory usage exceeds maximum size");
        }

        self.state = State::MemoryAllocated;
        self.is_compression_enabled = enable_compression;

        self.clear_storage();

        let storage_bytes = (num_bytes * self.num_storage) as usize;
        let storage_ip_bytes = (num_bytes * self.num_storage_ip) as usize;
        let storage_chance_bytes = (num_bytes * self.num_storage_chance) as usize;

        self.storage1 = vec![0; storage_bytes];
        self.storage2 = vec![0; storage_bytes];
        self.storage_ip = vec![0; storage_ip_bytes];
        self.storage_chance = vec![0; storage_chance_bytes];

        self.allocate_memory_nodes();

        self.storage_mode = BoardState::River;
        self.target_storage_mode = BoardState::River;
    }

    /// Checks the card configuration.
    pub(crate) fn check_card_config(&mut self) -> Result<(), String> {
        let config = &self.card_config;
        let (flop, turn, river) = (config.flop, config.turn, config.river);
        let range = &config.range;

        if flop.contains(&NOT_DEALT) {
            return Err("Flop cards not initialized".to_string());
        }

        if flop.iter().any(|&c| 52 <= c) {
            return Err(format!("Flop cards must be in [0, 52): flop = {flop:?}"));
        }

        if flop[0] == flop[1] || flop[0] == flop[2] || flop[1] == flop[2] {
            return Err(format!("Flop cards must be unique: flop = {flop:?}"));
        }

        if turn != NOT_DEALT {
            if 52 <= turn {
                return Err(format!("Turn card must be in [0, 52): turn = {turn}"));
            }

            if flop.contains(&turn) {
                return Err(format!(
                    "Turn card must be different from flop cards: turn = {turn}"
                ));
            }
        }

        if river != NOT_DEALT {
            if 52 <= river {
                return Err(format!("River card must be in [0, 52): river = {river}"));
            }

            if flop.contains(&river) {
                return Err(format!(
                    "River card must be different from flop cards: river = {river}"
                ));
            }

            if turn == river {
                return Err(format!(
                    "River card must be different from turn card: river = {river}"
                ));
            }

            if turn == NOT_DEALT {
                return Err(format!(
                    "River card specified without turn card: river = {river}"
                ));
            }
        }

        let expected_state = match (turn != NOT_DEALT, river != NOT_DEALT) {
            (false, _) => BoardState::Flop,
            (true, false) => BoardState::Turn,
            (true, true) => BoardState::River,
        };

        if self.tree_config.initial_state != expected_state {
            return Err(format!(
                "Invalid initial state of `tree_config`: expected = {:?}, actual = {:?}",
                expected_state, self.tree_config.initial_state
            ));
        }

        if range[0].is_empty() {
            return Err("OOP range is empty".to_string());
        }

        if range[1].is_empty() {
            return Err("IP range is empty".to_string());
        }

        if !range[0].is_valid() {
            return Err("OOP range is invalid (loaded broken data?)".to_string());
        }

        if !range[1].is_valid() {
            return Err("IP range is invalid (loaded broken data?)".to_string());
        }

        self.init_hands();
        self.num_combinations = 0.0;

        for (&(c1, c2), &w1) in self.private_cards[0]
            .iter()
            .zip(self.initial_weights[0].iter())
        {
            let oop_mask: u64 = (1 << c1) | (1 << c2);
            for (&(c3, c4), &w2) in self.private_cards[1]
                .iter()
                .zip(self.initial_weights[1].iter())
            {
                let ip_mask: u64 = (1 << c3) | (1 << c4);
                if oop_mask & ip_mask == 0 {
                    self.num_combinations += w1 as f64 * w2 as f64;
                }
            }
        }

        if self.num_combinations == 0.0 {
            return Err("Valid card assignment does not exist".to_string());
        }

        Ok(())
    }

    /// Initializes fields `initial_weights` and `private_cards`.
    #[inline]
    fn init_hands(&mut self) {
        let config = &self.card_config;
        let (flop, turn, river) = (config.flop, config.turn, config.river);
        let range = &config.range;

        let mut board_mask: u64 = (1 << flop[0]) | (1 << flop[1]) | (1 << flop[2]);
        if turn != NOT_DEALT {
            board_mask |= 1 << turn;
        }
        if river != NOT_DEALT {
            board_mask |= 1 << river;
        }

        for player in 0..2 {
            let (hands, weights) = range[player].get_hands_weights(board_mask);
            self.initial_weights[player] = weights;
            self.private_cards[player] = hands;
        }
    }

    /// Initializes fields related to cards.
    pub(super) fn init_card_fields(&mut self) {
        for player in 0..2 {
            let same_hand_index = &mut self.same_hand_index[player];
            same_hand_index.clear();

            let player_hands = &self.private_cards[player];
            let opponent_hands = &self.private_cards[player ^ 1];
            for hand in player_hands {
                same_hand_index.push(
                    opponent_hands
                        .binary_search(hand)
                        .map_or(u16::MAX, |i| i as u16),
                );
            }
        }

        (
            self.valid_indices_flop,
            self.valid_indices_turn,
            self.valid_indices_river,
        ) = self.card_config.valid_indices(&self.private_cards);

        self.hand_strength = self.card_config.hand_strength(&self.private_cards);

        (
            self.isomorphism_ref_turn,
            self.isomorphism_card_turn,
            self.isomorphism_swap_turn,
            self.isomorphism_ref_river,
            self.isomorphism_card_river,
            self.isomorphism_swap_river,
        ) = self.card_config.isomorphism(&self.private_cards);
    }

    /// Initializes the root node of game tree.
    fn init_root(&mut self) -> Result<(), String> {
        let num_nodes = self.count_num_nodes();
        let total_num_nodes = num_nodes[0] + num_nodes[1] + num_nodes[2];

        if total_num_nodes > u32::MAX as u64
            || mem::size_of::<PostFlopNode>() as u64 * total_num_nodes > isize::MAX as u64
        {
            return Err("Too many nodes".to_string());
        }

        self.num_nodes = num_nodes;
        self.node_arena = (0..total_num_nodes)
            .map(|_| MutexLike::new(PostFlopNode::default()))
            .collect::<Vec<_>>();
        self.clear_storage();

        let mut info = BuildTreeInfo {
            turn_index: num_nodes[0] as usize,
            river_index: (num_nodes[0] + num_nodes[1]) as usize,
            ..Default::default()
        };

        match self.tree_config.initial_state {
            BoardState::Flop => info.flop_index += 1,
            BoardState::Turn => info.turn_index += 1,
            BoardState::River => info.river_index += 1,
        }

        let mut root = self.node_arena[0].lock();
        root.turn = self.card_config.turn;
        root.river = self.card_config.river;

        self.build_tree_recursive(0, &self.action_root.lock(), &mut info);

        self.num_storage = info.num_storage;
        self.num_storage_ip = info.num_storage_ip;
        self.num_storage_chance = info.num_storage_chance;
        self.misc_memory_usage = self.memory_usage_internal();

        Ok(())
    }

    /// Initializes the interpreter.
    #[inline]
    pub(super) fn init_interpreter(&mut self) {
        let vecs = [
            vec![0.0; self.num_private_hands(0)],
            vec![0.0; self.num_private_hands(1)],
        ];

        self.weights = vecs.clone();
        self.normalized_weights = vecs.clone();
        self.cfvalues_cache = vecs;
    }

    /// Clears the storage.
    #[inline]
    fn clear_storage(&mut self) {
        self.storage1 = Vec::new();
        self.storage2 = Vec::new();
        self.storage_ip = Vec::new();
        self.storage_chance = Vec::new();
    }

    #[inline]
    fn clear_tournament_icm_internal(&mut self) {
        self.tournament_icm_config = None;
        self.terminal_icm_utilities = Vec::new();
        self.base_contribution = [0, 0];
    }

    fn precompute_terminal_icm_utilities(
        &self,
        config: &TournamentIcmConfig,
        base_contribution: [i32; 2],
        baseline_oop: f64,
        baseline_ip: f64,
    ) -> Result<Vec<Option<TerminalIcmUtility>>, String> {
        let mut utilities = vec![None; self.node_arena.len()];
        let mut utility_cache = BTreeMap::<[i32; 2], TerminalIcmUtility>::new();
        let effective_stack = self.tree_config.effective_stack;
        let mut stack = vec![(0, [effective_stack, effective_stack], 0)];

        while let Some((node_index, remaining, prev_amount)) = stack.pop() {
            let node = self.node_arena[node_index].lock();

            if node.is_terminal() {
                let contribution = [
                    effective_stack - remaining[0],
                    effective_stack - remaining[1],
                ];

                for player in 0..2 {
                    if contribution[player] < 0 {
                        return Err(format!(
                            "Tournament ICM terminal contribution for player {player} is negative"
                        ));
                    }

                    let seat = if player == 0 {
                        config.oop_seat
                    } else {
                        config.ip_seat
                    };

                    let total_contribution = base_contribution[player] + contribution[player];
                    if config.stacks[seat] < total_contribution as f64 {
                        return Err(format!(
                            "Tournament ICM active stack for player {player} cannot cover terminal contribution {total_contribution}"
                        ));
                    }
                }

                let utility = if let Some(&utility) = utility_cache.get(&contribution) {
                    utility
                } else {
                    let oop_win = self.tournament_icm_values_for_terminal(
                        config,
                        contribution,
                        base_contribution,
                        Some(0),
                    )?;
                    let ip_win = self.tournament_icm_values_for_terminal(
                        config,
                        contribution,
                        base_contribution,
                        Some(1),
                    )?;
                    let tie = self.tournament_icm_values_for_terminal(
                        config,
                        contribution,
                        base_contribution,
                        None,
                    )?;

                    let utility = TerminalIcmUtility {
                        win: [
                            (oop_win[config.oop_seat] - baseline_oop) as f32,
                            (ip_win[config.ip_seat] - baseline_ip) as f32,
                        ],
                        lose: [
                            (ip_win[config.oop_seat] - baseline_oop) as f32,
                            (oop_win[config.ip_seat] - baseline_ip) as f32,
                        ],
                        tie: [
                            (tie[config.oop_seat] - baseline_oop) as f32,
                            (tie[config.ip_seat] - baseline_ip) as f32,
                        ],
                    };
                    utility_cache.insert(contribution, utility);
                    utility
                };

                utilities[node_index] = Some(utility);

                continue;
            }

            let player = (node.player & PLAYER_MASK) as usize;
            for offset in 0..node.num_children as usize {
                let child_index = node_index + node.children_offset as usize + offset;
                if child_index >= self.node_arena.len() {
                    // Turn/Flop-mode save: the river subtree was dropped from the arena, so the
                    // child pointer dangles past the end. Those children are unreachable for ICM
                    // (the precompute only needs resident terminals), so skip them.
                    continue;
                }
                let action = self.node_arena[child_index].lock().prev_action;
                let (next_remaining, next_prev_amount) =
                    Self::next_icm_contribution_state(player, action, remaining, prev_amount)?;
                stack.push((child_index, next_remaining, next_prev_amount));
            }
        }

        Ok(utilities)
    }

    fn next_icm_contribution_state(
        player: usize,
        action: Action,
        mut remaining: [i32; 2],
        mut prev_amount: i32,
    ) -> Result<([i32; 2], i32), String> {
        match action {
            Action::Check | Action::Chance(_) | Action::Fold | Action::None => {}
            Action::Call => {
                if player >= 2 {
                    return Err("Tournament ICM call action has invalid player".to_string());
                }
                remaining[player] = remaining[player ^ 1];
                prev_amount = 0;
            }
            Action::Bet(amount) | Action::Raise(amount) | Action::AllIn(amount) => {
                if player >= 2 {
                    return Err("Tournament ICM bet action has invalid player".to_string());
                }
                let to_call = remaining[player] - remaining[player ^ 1];
                let delta = amount - prev_amount + to_call;
                if delta < 0 {
                    return Err(format!(
                        "Tournament ICM contribution delta is negative for {action:?}"
                    ));
                }
                remaining[player] -= delta;
                if remaining[player] < 0 {
                    return Err(format!(
                        "Tournament ICM contribution for player {player} exceeds effective stack after {action:?}"
                    ));
                }
                prev_amount = amount;
            }
        }

        Ok((remaining, prev_amount))
    }

    /// Computes the ICM values at a terminal outcome.
    ///
    /// `contribution` is the per-player investment made WITHIN this game's tree (measured from
    /// `effective_stack`). `base_contribution` is the per-player investment made BEFORE this game
    /// began — non-zero only for re-rooted river games whose `starting_pot` already folds in the
    /// pre-river pot. The winner formula deducts the FULL investment (`base + contribution`) from
    /// each seat's absolute tournament stack, while the pot is built from `contribution` only
    /// (the re-rooted `starting_pot` already contains `base`, so adding it again would
    /// double-count). With `base_contribution == [0, 0]` this is byte-identical to the legacy
    /// formula.
    fn tournament_icm_values_for_terminal(
        &self,
        config: &TournamentIcmConfig,
        contribution: [i32; 2],
        base_contribution: [i32; 2],
        winner: Option<usize>,
    ) -> Result<Vec<f64>, String> {
        let mut stacks = config.stacks.clone();
        let starting_pot = self.tree_config.starting_pot as f64;
        let c = [contribution[0] as f64, contribution[1] as f64];
        let b = [base_contribution[0] as f64, base_contribution[1] as f64];
        let pot = starting_pot + c[0] + c[1]; // UNCHANGED: base already lives in starting_pot
        let total = [b[0] + c[0], b[1] + c[1]]; // full-hand deduction

        match winner {
            Some(0) => {
                stacks[config.oop_seat] += pot - total[0];
                stacks[config.ip_seat] -= total[1];
            }
            Some(1) => {
                stacks[config.ip_seat] += pot - total[1];
                stacks[config.oop_seat] -= total[0];
            }
            Some(_) => unreachable!(),
            None => {
                stacks[config.oop_seat] += pot * 0.5 - total[0];
                stacks[config.ip_seat] += pot * 0.5 - total[1];
            }
        }

        terminal_icm_values(&stacks, &config.payouts)
    }

    /// Counts the number of nodes in the game tree.
    #[inline]
    fn count_num_nodes(&self) -> [u64; 3] {
        let (turn_coef, river_coef) = match (self.card_config.turn, self.card_config.river) {
            (NOT_DEALT, _) => {
                let mut river_coef = 0;
                let flop = self.card_config.flop;
                let skip_cards = &self.isomorphism_card_turn;
                let flop_mask: u64 = (1 << flop[0]) | (1 << flop[1]) | (1 << flop[2]);
                let skip_mask: u64 = skip_cards.iter().map(|&card| 1 << card).sum();
                for turn in 0..52 {
                    if (1 << turn) & (flop_mask | skip_mask) == 0 {
                        river_coef += 48 - self.isomorphism_card_river[turn & 3].len();
                    }
                }
                (49 - self.isomorphism_card_turn.len(), river_coef)
            }
            (turn, NOT_DEALT) => (1, 48 - self.isomorphism_card_river[turn as usize & 3].len()),
            _ => (0, 1),
        };

        let num_action_nodes = count_num_action_nodes(&self.action_root.lock());

        [
            num_action_nodes[0],
            num_action_nodes[1] * turn_coef as u64,
            num_action_nodes[2] * river_coef as u64,
        ]
    }

    /// Computes the memory usage of this struct.
    #[inline]
    fn memory_usage_internal(&self) -> u64 {
        // untracked: tree_config, action_root

        let mut memory_usage = mem::size_of::<Self>() as u64;

        memory_usage += vec_memory_usage(&self.added_lines);
        memory_usage += vec_memory_usage(&self.removed_lines);
        for line in &self.added_lines {
            memory_usage += vec_memory_usage(line);
        }
        for line in &self.removed_lines {
            memory_usage += vec_memory_usage(line);
        }

        memory_usage += vec_memory_usage(&self.valid_indices_turn);
        memory_usage += vec_memory_usage(&self.valid_indices_river);
        memory_usage += vec_memory_usage(&self.hand_strength);
        memory_usage += vec_memory_usage(&self.isomorphism_ref_turn);
        memory_usage += vec_memory_usage(&self.isomorphism_card_turn);
        memory_usage += vec_memory_usage(&self.isomorphism_ref_river);

        for refs in &self.isomorphism_ref_river {
            memory_usage += vec_memory_usage(refs);
        }

        for cards in &self.isomorphism_card_river {
            memory_usage += vec_memory_usage(cards);
        }

        for player in 0..2 {
            memory_usage += vec_memory_usage(&self.initial_weights[player]);
            memory_usage += vec_memory_usage(&self.private_cards[player]);
            memory_usage += vec_memory_usage(&self.same_hand_index[player]);
            memory_usage += vec_memory_usage(&self.valid_indices_flop[player]);
            for indices in &self.valid_indices_turn {
                memory_usage += vec_memory_usage(&indices[player]);
            }
            for indices in &self.valid_indices_river {
                memory_usage += vec_memory_usage(&indices[player]);
            }
            for strength in &self.hand_strength {
                memory_usage += vec_memory_usage(&strength[player]);
            }
            for swap in &self.isomorphism_swap_turn {
                memory_usage += vec_memory_usage(&swap[player]);
            }
            for swap_list in &self.isomorphism_swap_river {
                for swap in swap_list {
                    memory_usage += vec_memory_usage(&swap[player]);
                }
            }
        }

        memory_usage += vec_memory_usage(&self.node_arena);

        memory_usage
    }

    /// Builds the game tree recursively.
    fn build_tree_recursive(
        &self,
        node_index: usize,
        action_node: &ActionTreeNode,
        info: &mut BuildTreeInfo,
    ) {
        let mut node = self.node_arena[node_index].lock();
        node.player = action_node.player;
        node.amount = action_node.amount;

        if node.is_terminal() {
            return;
        }

        if node.is_chance() {
            self.push_chances(node_index, info);
            for action_index in 0..node.num_actions() {
                let child_index = node_index + node.children_offset as usize + action_index;
                self.build_tree_recursive(child_index, &action_node.children[0].lock(), info);
            }
        } else {
            self.push_actions(node_index, action_node, info);
            for action_index in 0..node.num_actions() {
                let child_index = node_index + node.children_offset as usize + action_index;
                self.build_tree_recursive(
                    child_index,
                    &action_node.children[action_index].lock(),
                    info,
                );
            }
        }
    }

    /// Pushes the chance actions to the `node`.
    fn push_chances(&self, node_index: usize, info: &mut BuildTreeInfo) {
        let mut node = self.node_arena[node_index].lock();
        let flop = self.card_config.flop;
        let flop_mask: u64 = (1 << flop[0]) | (1 << flop[1]) | (1 << flop[2]);

        // deal turn
        if node.turn == NOT_DEALT {
            let skip_cards = &self.isomorphism_card_turn;
            let skip_mask: u64 = skip_cards.iter().map(|&card| 1 << card).sum();

            node.children_offset = (info.turn_index - node_index) as u32;
            for card in 0..52 {
                if (1 << card) & (flop_mask | skip_mask) == 0 {
                    node.num_children += 1;
                    let mut child = node.children().last().unwrap().lock();
                    child.prev_action = Action::Chance(card);
                    child.turn = card;
                }
            }

            info.turn_index += node.num_children as usize;
        }
        // deal river
        else {
            let turn_mask = flop_mask | (1 << node.turn);
            let skip_cards = &self.isomorphism_card_river[node.turn as usize & 3];
            let skip_mask: u64 = skip_cards.iter().map(|&card| 1 << card).sum();

            node.children_offset = (info.river_index - node_index) as u32;
            for card in 0..52 {
                if (1 << card) & (turn_mask | skip_mask) == 0 {
                    node.num_children += 1;
                    let mut child = node.children().last().unwrap().lock();
                    child.prev_action = Action::Chance(card);
                    child.turn = node.turn;
                    child.river = card;
                }
            }

            info.river_index += node.num_children as usize;
        }

        node.num_elements = node
            .cfvalue_storage_player()
            .map_or(0, |player| self.num_private_hands(player)) as u32;

        info.num_storage_chance += node.num_elements as u64;
    }

    /// Pushes the actions to the `node`.
    fn push_actions(
        &self,
        node_index: usize,
        action_node: &ActionTreeNode,
        info: &mut BuildTreeInfo,
    ) {
        let mut node = self.node_arena[node_index].lock();

        let street = match (node.turn, node.river) {
            (NOT_DEALT, _) => BoardState::Flop,
            (_, NOT_DEALT) => BoardState::Turn,
            _ => BoardState::River,
        };

        let base = match street {
            BoardState::Flop => &mut info.flop_index,
            BoardState::Turn => &mut info.turn_index,
            BoardState::River => &mut info.river_index,
        };

        node.children_offset = (*base - node_index) as u32;
        node.num_children = action_node.children.len() as u16;
        *base += node.num_children as usize;

        for (child, action) in node.children().iter().zip(action_node.actions.iter()) {
            let mut child = child.lock();
            child.prev_action = *action;
            child.turn = node.turn;
            child.river = node.river;
        }

        let num_private_hands = self.num_private_hands(node.player as usize);
        node.num_elements = (node.num_actions() * num_private_hands) as u32;
        node.num_elements_ip = match node.prev_action {
            Action::None | Action::Chance(_) => self.num_private_hands(PLAYER_IP as usize) as u16,
            _ => 0,
        };

        info.num_storage += node.num_elements as u64;
        info.num_storage_ip += node.num_elements_ip as u64;
    }

    /// Sets the bunching effect.
    fn set_bunching_effect_internal(&mut self, bunching_data: &BunchingData) -> Result<(), String> {
        self.bunching_num_dead_cards = bunching_data.fold_ranges().len() * 2;
        let mut arena = vec![0.0]; // store dummy element

        // hand strength
        self.bunching_strength = self
            .hand_strength
            .iter()
            .map(|strength| {
                if strength[0].is_empty() {
                    return [Vec::new(), Vec::new()];
                }

                let mut ret = [
                    vec![0; self.num_private_hands(0)],
                    vec![0; self.num_private_hands(1)],
                ];

                for player in 0..2 {
                    let len = strength[player].len();
                    for &item in &strength[player][1..len - 1] {
                        ret[player][item.index as usize] = item.strength;
                    }
                }

                ret
            })
            .collect();

        // flop num combinations
        if self.card_config.turn == NOT_DEALT {
            for player in 0..2 {
                let player_cards = &self.private_cards[player];
                let opponent_cards = &self.private_cards[player ^ 1];
                let mut indices = Vec::with_capacity(player_cards.len());

                for &(c1, c2) in player_cards {
                    indices.push(arena.len());
                    let player_mask: u64 = (1 << c1) | (1 << c2);

                    for &(c3, c4) in opponent_cards {
                        let opponent_mask: u64 = (1 << c3) | (1 << c4);
                        if player_mask & opponent_mask != 0 {
                            arena.push(0.0);
                        } else {
                            let mask = player_mask | opponent_mask;
                            arena.push(bunching_data.result_4cards(mask));
                        }
                    }
                }

                if player == 0 {
                    self.bunching_num_combinations = arena.iter().fold(0.0, |a, &x| a + x as f64);
                    if self.bunching_num_combinations == 0.0 {
                        self.reset_bunching_effect();
                        return Err("Valid combination not found".to_string());
                    }
                }

                self.bunching_num_flop[player] = indices;
            }
        }

        let flop_mask: u64 = self.card_config.flop.iter().map(|&c| 1 << c).sum();
        let skip_turn_mask: u64 = self.isomorphism_card_turn.iter().map(|&c| 1 << c).sum();

        // turn num combinations
        if self.card_config.river == NOT_DEALT {
            for player in 0..2 {
                let player_cards = &self.private_cards[player];
                let opponent_cards = &self.private_cards[player ^ 1];

                let buf = into_par_iter(0..52)
                    .map(|turn| {
                        let bit_turn: u64 = 1 << turn;
                        if bit_turn & (flop_mask | skip_turn_mask) != 0
                            || (self.card_config.turn != NOT_DEALT
                                && self.card_config.turn != turn as Card)
                        {
                            return Vec::new();
                        }

                        let mut outer = Vec::with_capacity(player_cards.len());

                        for &(c1, c2) in player_cards {
                            let player_mask: u64 = (1 << c1) | (1 << c2);
                            if player_mask & bit_turn != 0 {
                                outer.push(Vec::new());
                                continue;
                            }

                            let mut inner = Vec::with_capacity(opponent_cards.len());
                            for &(c3, c4) in opponent_cards {
                                let opponent_mask: u64 = (1 << c3) | (1 << c4);
                                if (player_mask | bit_turn) & opponent_mask != 0 {
                                    inner.push(0.0);
                                } else {
                                    let mask = player_mask | opponent_mask | bit_turn;
                                    inner.push(bunching_data.result_5cards(mask));
                                }
                            }

                            outer.push(inner);
                        }

                        outer
                    })
                    .collect::<Vec<_>>();

                self.bunching_num_turn[player] = Self::push_vec_to_arena_f32(&mut arena, buf);

                if self.card_config.turn != NOT_DEALT && player == 0 {
                    self.bunching_num_combinations = arena.iter().fold(0.0, |a, &x| a + x as f64);
                    if self.bunching_num_combinations == 0.0 {
                        self.reset_bunching_effect();
                        return Err("Valid combination not found".to_string());
                    }
                }
            }
        }

        let is_board_possible = |turn: Card, river: Card| {
            let bit_turn: u64 = 1 << turn;
            let bit_river: u64 = 1 << river;
            let iso_card = &self.isomorphism_card_river[turn as usize & 3];

            bit_turn & (flop_mask | skip_turn_mask) == 0
                && bit_river & flop_mask == 0
                && !iso_card.contains(&river)
                && (self.card_config.turn == NOT_DEALT || self.card_config.turn == turn)
                && (self.card_config.river == NOT_DEALT || self.card_config.river == river)
        };

        // river num combinations
        for player in 0..2 {
            let player_cards = &self.private_cards[player];
            let opponent_cards = &self.private_cards[player ^ 1];

            let buf = into_par_iter(0..52 * 51 / 2)
                .map(|index| {
                    let (board1, board2) = index_to_card_pair(index);
                    if !is_board_possible(board1, board2) && !is_board_possible(board2, board1) {
                        return Vec::new();
                    }

                    let board_mask: u64 = (1 << board1) | (1 << board2);
                    let mut outer = Vec::with_capacity(player_cards.len());

                    for &(c1, c2) in player_cards {
                        let player_mask: u64 = (1 << c1) | (1 << c2);
                        if player_mask & board_mask != 0 {
                            outer.push(Vec::new());
                            continue;
                        }

                        let mut inner = Vec::with_capacity(opponent_cards.len());
                        for &(c3, c4) in opponent_cards {
                            let opponent_mask: u64 = (1 << c3) | (1 << c4);
                            if (player_mask | board_mask) & opponent_mask != 0 {
                                inner.push(0.0);
                            } else {
                                let mask = player_mask | opponent_mask | board_mask;
                                inner.push(bunching_data.result_6cards(mask));
                            }
                        }

                        outer.push(inner);
                    }

                    outer
                })
                .collect::<Vec<_>>();

            self.bunching_num_river[player] = Self::push_vec_to_arena_f32(&mut arena, buf);

            if self.card_config.river != NOT_DEALT && player == 0 {
                self.bunching_num_combinations = arena.iter().fold(0.0, |a, &x| a + x as f64);
                if self.bunching_num_combinations == 0.0 {
                    self.reset_bunching_effect();
                    return Err("Valid combination not found".to_string());
                }
            }
        }

        if self.card_config.river != NOT_DEALT {
            self.bunching_arena = arena;
            self.assign_zero_weights();
            return Ok(());
        }

        // turn equity coefficients
        for player in 0..2 {
            let player_cards = &self.private_cards[player];
            let opponent_cards = &self.private_cards[player ^ 1];
            let player_len = player_cards.len();
            let opponent_len = opponent_cards.len();

            let buf = into_par_iter(0..52)
                .map(|turn| {
                    let bit_turn: u64 = 1 << turn;
                    if bit_turn & (flop_mask | skip_turn_mask) != 0
                        || (self.card_config.turn != NOT_DEALT
                            && self.card_config.turn != turn as Card)
                    {
                        return Vec::new();
                    }

                    let mut outer = Vec::with_capacity(player_len);

                    for &(c1, c2) in player_cards {
                        let player_mask: u64 = (1 << c1) | (1 << c2);
                        if player_mask & bit_turn != 0 {
                            outer.push(Vec::new());
                        } else {
                            outer.push(vec![0.0; opponent_len]);
                        }
                    }

                    let mut children = Vec::with_capacity(48);
                    let iso_ref = &self.isomorphism_ref_river[turn];
                    let iso_card = &self.isomorphism_card_river[turn & 3];
                    let iso_swap = &self.isomorphism_swap_river[turn & 3];

                    for river in 0..52 {
                        let bit_river: u64 = 1 << river;
                        if bit_river & (flop_mask | bit_turn) != 0 {
                            continue;
                        }

                        let pos = iso_card.iter().position(|&c| c == river);
                        let (river_ref, swap_option) = if let Some(pos) = pos {
                            let child_index = iso_ref[pos] as usize;
                            let swap = &iso_swap[river as usize & 3];
                            (children[child_index], Some(swap))
                        } else {
                            children.push(river);
                            (river, None)
                        };

                        let player_swap = swap_option.map(|swap| {
                            let mut tmp = (0..player_len).collect::<Vec<_>>();
                            apply_swap(&mut tmp, &swap[player]);
                            tmp
                        });

                        let pair_index = card_pair_to_index(turn as Card, river_ref);
                        let arena_indices = &self.bunching_num_river[player][pair_index];
                        let player_strength = &self.bunching_strength[pair_index][player];
                        let opponent_strength = &self.bunching_strength[pair_index][player ^ 1];

                        for (i, inner) in outer.iter_mut().enumerate() {
                            let player_index = player_swap.as_ref().map_or(i, |map| map[i]);
                            let index = arena_indices[player_index];
                            let threshold = player_strength[player_index];

                            if index == 0 {
                                continue;
                            }

                            let mut tmp = (Vec::new(), Vec::new());
                            let slices = if let Some(swap) = swap_option {
                                tmp.0.extend_from_slice(&arena[index..index + opponent_len]);
                                tmp.1.extend_from_slice(opponent_strength);
                                apply_swap(&mut tmp.0, &swap[player ^ 1]);
                                apply_swap(&mut tmp.1, &swap[player ^ 1]);
                                (tmp.0.as_slice(), &tmp.1)
                            } else {
                                (&arena[index..index + opponent_len], opponent_strength)
                            };

                            inner.iter_mut().zip(slices.0).zip(slices.1).for_each(
                                |((dst, num), &strength)| {
                                    #[allow(clippy::comparison_chain)]
                                    if strength < threshold {
                                        *dst += *num as f64;
                                    } else if strength > threshold {
                                        *dst += -*num as f64;
                                    } else {
                                        *dst += 0.0;
                                    }
                                },
                            );
                        }
                    }

                    let num_possible_river = (44 - self.bunching_num_dead_cards) as f64;
                    outer.iter_mut().for_each(|inner| {
                        inner.iter_mut().for_each(|c| {
                            *c /= num_possible_river;
                        });
                    });

                    outer
                })
                .collect::<Vec<_>>();

            self.bunching_coef_turn[player] = Self::push_vec_to_arena_f64(&mut arena, buf);
        }

        if self.card_config.turn != NOT_DEALT {
            self.bunching_arena = arena;
            self.assign_zero_weights();
            return Ok(());
        }

        // flop equity coefficients
        for player in 0..2 {
            let player_cards = &self.private_cards[player];
            let opponent_cards = &self.private_cards[player ^ 1];
            let player_len = player_cards.len();
            let opponent_len = opponent_cards.len();

            let mut outer = vec![vec![0.0; opponent_len]; player_len];
            let mut children = Vec::with_capacity(49);

            for turn in 0..52 {
                let bit_turn: u64 = 1 << turn;
                if bit_turn & flop_mask != 0 {
                    continue;
                }

                let iso_card = &self.isomorphism_card_turn;
                let pos = iso_card.iter().position(|&c| c == turn as Card);

                let (turn_ref, swap_option) = if let Some(pos) = pos {
                    let child_index = self.isomorphism_ref_turn[pos] as usize;
                    let swap = &self.isomorphism_swap_turn[turn & 3];
                    (children[child_index], Some(swap))
                } else {
                    children.push(turn);
                    (turn, None)
                };

                let player_swap = swap_option.map(|swap| {
                    let mut tmp = (0..player_len).collect::<Vec<_>>();
                    apply_swap(&mut tmp, &swap[player]);
                    tmp
                });

                let arena_indices = &self.bunching_coef_turn[player][turn_ref];

                for (i, inner) in outer.iter_mut().enumerate() {
                    let player_index = player_swap.as_ref().map_or(i, |map| map[i]);
                    let index = arena_indices[player_index];
                    if index == 0 {
                        continue;
                    }

                    let mut tmp = Vec::new();
                    let slice = &arena[index..index + opponent_len];
                    let slice = if let Some(swap) = swap_option {
                        tmp.extend_from_slice(slice);
                        apply_swap(&mut tmp, &swap[player ^ 1]);
                        &tmp
                    } else {
                        slice
                    };

                    inner.iter_mut().zip(slice).for_each(|(dst, &num)| {
                        *dst += num as f64;
                    });
                }
            }

            let num_possible_turn = (45 - self.bunching_num_dead_cards) as f64;
            outer.iter_mut().for_each(|inner| {
                inner.iter_mut().for_each(|c| {
                    *c /= num_possible_turn;
                });
            });

            Self::push_vec_to_arena_f64(&mut arena, vec![outer])
                .into_iter()
                .for_each(|v| {
                    self.bunching_coef_flop[player] = v;
                });
        }

        self.bunching_arena = arena;
        self.assign_zero_weights();
        Ok(())
    }

    /// Sets the bunching effect.
    fn memory_usage_bunching_internal(&self) -> u64 {
        let mut ret = 4;

        let oop_len = self.num_private_hands(0);
        let ip_len = self.num_private_hands(1);

        // hand strength
        self.hand_strength.iter().for_each(|strength| {
            ret += mem::size_of::<[Vec<u16>; 2]>() as u64;
            if !strength[0].is_empty() {
                ret += 2 * (oop_len + ip_len) as u64;
            }
        });

        // flop num combinations / equity coefficients
        if self.card_config.turn == NOT_DEALT {
            ret += 2 * (oop_len * ip_len * mem::size_of::<usize>()) as u64;
            ret += 2 * 2 * 4 * (oop_len * ip_len) as u64;
        }

        let flop_mask: u64 = self.card_config.flop.iter().map(|&c| 1 << c).sum();
        let skip_turn_mask: u64 = self.isomorphism_card_turn.iter().map(|&c| 1 << c).sum();

        // turn num combinations / equity coefficients
        if self.card_config.river == NOT_DEALT {
            for player in 0..2 {
                let player_cards = &self.private_cards[player];
                let opponent_cards = &self.private_cards[player ^ 1];
                let player_len = player_cards.len();
                let opponent_len = opponent_cards.len();

                ret += 2 * 52 * mem::size_of::<Vec<usize>>() as u64;

                for turn in 0..52 {
                    let bit_turn: u64 = 1 << turn;
                    if bit_turn & (flop_mask | skip_turn_mask) != 0
                        || (self.card_config.turn != NOT_DEALT
                            && self.card_config.turn != turn as Card)
                    {
                        continue;
                    }

                    ret += 2 * (player_len * mem::size_of::<usize>()) as u64;

                    for &(c1, c2) in player_cards {
                        let player_mask: u64 = (1 << c1) | (1 << c2);
                        if player_mask & bit_turn == 0 {
                            ret += 2 * 4 * opponent_len as u64;
                        }
                    }
                }
            }
        }

        let is_board_possible = |turn: Card, river: Card| {
            let bit_turn: u64 = 1 << turn;
            let bit_river: u64 = 1 << river;
            let iso_card = &self.isomorphism_card_river[turn as usize & 3];

            bit_turn & (flop_mask | skip_turn_mask) == 0
                && bit_river & flop_mask == 0
                && !iso_card.contains(&river)
                && (self.card_config.turn == NOT_DEALT || self.card_config.turn == turn)
                && (self.card_config.river == NOT_DEALT || self.card_config.river == river)
        };

        // river num combinations
        for player in 0..2 {
            let player_cards = &self.private_cards[player];
            let opponent_cards = &self.private_cards[player ^ 1];
            let player_len = player_cards.len();
            let opponent_len = opponent_cards.len();

            ret += 52 * 51 / 2 * mem::size_of::<Vec<usize>>() as u64;

            for index in 0..52 * 51 / 2 {
                let (board1, board2) = index_to_card_pair(index);
                if !is_board_possible(board1, board2) && !is_board_possible(board2, board1) {
                    continue;
                }

                let board_mask: u64 = (1 << board1) | (1 << board2);
                ret += (player_len * mem::size_of::<usize>()) as u64;

                for &(c1, c2) in player_cards {
                    let player_mask: u64 = (1 << c1) | (1 << c2);
                    if player_mask & board_mask == 0 {
                        ret += 4 * opponent_len as u64;
                    }
                }
            }
        }

        ret
    }

    /// Pushes a nested `Vec` to an arena and returns the indices of the pushed elements.
    fn push_vec_to_arena_f32(arena: &mut Vec<f32>, vec: Vec<Vec<Vec<f32>>>) -> Vec<Vec<usize>> {
        let mut ret = Vec::with_capacity(vec.len());

        for outer in vec.into_iter() {
            let mut indices = Vec::with_capacity(outer.len());

            for inner in outer.into_iter() {
                if inner.is_empty() {
                    indices.push(0);
                } else {
                    indices.push(arena.len());
                    arena.extend(inner);
                }
            }

            ret.push(indices);
        }

        ret
    }

    /// Pushes a nested `Vec` to an arena and returns the indices of the pushed elements.
    fn push_vec_to_arena_f64(arena: &mut Vec<f32>, vec: Vec<Vec<Vec<f64>>>) -> Vec<Vec<usize>> {
        let mut ret = Vec::with_capacity(vec.len());

        for outer in vec.into_iter() {
            let mut indices = Vec::with_capacity(outer.len());

            for inner in outer.into_iter() {
                if inner.is_empty() {
                    indices.push(0);
                } else {
                    indices.push(arena.len());
                    arena.extend(inner.into_iter().map(|v| v as f32));
                }
            }

            ret.push(indices);
        }

        ret
    }

    /// Calculates the number of storage elements that will be removed.
    fn calculate_removed_line_info_recursive(node: &mut PostFlopNode, info: &mut BuildTreeInfo) {
        if node.is_terminal() {
            return;
        }

        if node.is_chance() {
            info.num_storage_chance += node.num_elements as u64;
            node.num_elements = 0;
        } else {
            info.num_storage += node.num_elements as u64;
            info.num_storage_ip += node.num_elements_ip as u64;
            node.num_elements = 0;
            node.num_elements_ip = 0;
        }

        for action in node.action_indices() {
            Self::calculate_removed_line_info_recursive(&mut node.play(action), info);
        }
    }

    /// Remove a line from a `PostFlopGame` tree.
    fn remove_line_recursive(
        &self,
        node: &mut PostFlopNode,
        line: &[Action],
    ) -> Result<BuildTreeInfo, String> {
        if line.is_empty() {
            return Err("Empty line".to_string());
        }

        if node.is_terminal() {
            return Err("Unexpected terminal node".to_string());
        }

        let action = line[0];
        let search_result = node
            .children()
            .binary_search_by(|child| child.lock().prev_action.cmp(&action));

        if search_result.is_err() {
            return Err(format!("Action does not exist: {action:?}"));
        }

        let index = search_result.unwrap();
        if line.len() > 1 {
            let result = self.remove_line_recursive(&mut node.children()[index].lock(), &line[1..]);
            return result;
        }

        if node.is_chance() {
            return Err("Cannot remove a line ending in a chance action".to_string());
        }

        if node.num_actions() <= 1 {
            return Err("Cannot remove the last action from a node".to_string());
        }

        // Remove action/children at index. To do this we must
        // 1. compute the storage space required by the tree rooted at action index
        // 2. remove children and actions
        // 3. re-define `num_elements` after we remove children and actions

        // STEP 1
        let mut info = BuildTreeInfo {
            num_storage: self.num_private_hands(node.player as usize) as u64,
            ..Default::default()
        };

        let mut node_to_remove = node.play(index);
        Self::calculate_removed_line_info_recursive(&mut node_to_remove, &mut info);

        // STEP 2
        let children = node.children();
        for i in index..node.num_children as usize - 1 {
            let mut x = children[i].lock();
            let mut y = children[i + 1].lock();
            mem::swap(&mut *x, &mut *y);
            if x.children_offset > 0 {
                x.children_offset += 1;
            }
        }
        node.num_children -= 1;

        // STEP 3
        node.num_elements -= self.num_private_hands(node.player as usize) as u32;

        Ok(info)
    }

    /// Allocates memory recursively.
    fn allocate_memory_nodes(&mut self) {
        let num_bytes = if self.is_compression_enabled { 2 } else { 4 };
        let mut action_counter = 0;
        let mut ip_counter = 0;
        let mut chance_counter = 0;

        for node in &self.node_arena {
            let mut node = node.lock();
            if node.is_terminal() {
                // do nothing
            } else if node.is_chance() {
                unsafe {
                    let ptr = self.storage_chance.as_mut_ptr();
                    node.storage1 = ptr.add(chance_counter);
                }
                chance_counter += num_bytes * node.num_elements as usize;
            } else {
                unsafe {
                    let ptr1 = self.storage1.as_mut_ptr();
                    let ptr2 = self.storage2.as_mut_ptr();
                    let ptr3 = self.storage_ip.as_mut_ptr();
                    node.storage1 = ptr1.add(action_counter);
                    node.storage2 = ptr2.add(action_counter);
                    node.storage3 = ptr3.add(ip_counter);
                }
                action_counter += num_bytes * node.num_elements as usize;
                ip_counter += num_bytes * node.num_elements_ip as usize;
            }
        }
    }
}

#[cfg(test)]
mod icm_offset_tests {
    use super::*;
    use crate::range::{flop_from_str, Range};

    /// Builds a minimal flop game whose `tree_config.starting_pot` can be mutated freely so we can
    /// exercise `tournament_icm_values_for_terminal` directly (it only reads `starting_pot` from
    /// `tree_config` plus the provided `config`/`contribution`/`base_contribution`). No solve and no
    /// memory allocation are required.
    fn minimal_game() -> PostFlopGame {
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
        PostFlopGame::with_config(card_config, action_tree).unwrap()
    }

    fn icm_config() -> TournamentIcmConfig {
        // Concrete absolute tournament stacks/payouts/seats. Both players hold enough chips to
        // cover the contributions used below so the :837 guard does not trip in the precompute,
        // but here we call `tournament_icm_values_for_terminal` directly (no guard) anyway.
        TournamentIcmConfig {
            stacks: vec![1500.0, 1200.0, 800.0],
            payouts: vec![50, 30, 20],
            oop_seat: 0,
            ip_seat: 1,
        }
    }

    fn approx_eq(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    /// The SAME physical hand expressed two ways must yield IDENTICAL ICM equities:
    ///   FULL:    starting_pot = parent_pot,        contribution = [P+rc0, P+rc1], base = [0, 0]
    ///   REROOT:  starting_pot = parent_pot + 2P,   contribution = [rc0, rc1],     base = [P, P]
    /// The re-rooted `starting_pot` already folds in the pre-river pot (2P), so the offset must
    /// reconstruct the exact full-hand terminal stacks for every winner branch.
    #[test]
    fn reroot_offset_matches_full_hand_for_every_winner() {
        let parent_pot: i32 = 40; // pot at the root of the FULL hand (before P is invested)
        let p: i32 = 25; // pre-river investment per seat
        let rc0: i32 = 60; // river-local investment, OOP
        let rc1: i32 = 80; // river-local investment, IP

        let mut game = minimal_game();
        let config = icm_config();

        for winner in [Some(0usize), Some(1usize), None] {
            // FULL: the whole hand lives in this tree; nothing was invested before it.
            game.tree_config.starting_pot = parent_pot;
            let full = game
                .tournament_icm_values_for_terminal(
                    &config,
                    [p + rc0, p + rc1],
                    [0, 0],
                    winner,
                )
                .unwrap();

            // REROOT: the river is re-rooted; starting_pot already contains the pre-river pot (2P),
            // and `base = [P, P]` carries the pre-river investment so the full deduction is correct.
            game.tree_config.starting_pot = parent_pot + 2 * p;
            let reroot = game
                .tournament_icm_values_for_terminal(&config, [rc0, rc1], [p, p], winner)
                .unwrap();

            assert!(
                approx_eq(&full, &reroot),
                "winner {winner:?}: FULL {full:?} != REROOT {reroot:?}"
            );
        }
    }

    /// Negative control: a re-rooted game with `base = [0, 0]` (the buggy path) must DIFFER from the
    /// FULL expression, proving the offset is load-bearing rather than cosmetic. Without the base,
    /// the re-rooted game forgets the pre-river investment and overstates both final stacks by P.
    #[test]
    fn reroot_without_base_differs_from_full() {
        let parent_pot: i32 = 40;
        let p: i32 = 25;
        let rc0: i32 = 60;
        let rc1: i32 = 80;

        let mut game = minimal_game();
        let config = icm_config();

        for winner in [Some(0usize), Some(1usize), None] {
            game.tree_config.starting_pot = parent_pot;
            let full = game
                .tournament_icm_values_for_terminal(
                    &config,
                    [p + rc0, p + rc1],
                    [0, 0],
                    winner,
                )
                .unwrap();

            // Re-rooted starting_pot but WITHOUT the base offset: the bug.
            game.tree_config.starting_pot = parent_pot + 2 * p;
            let buggy = game
                .tournament_icm_values_for_terminal(&config, [rc0, rc1], [0, 0], winner)
                .unwrap();

            assert!(
                !approx_eq(&full, &buggy),
                "winner {winner:?}: offset was cosmetic, FULL {full:?} == BUGGY {buggy:?}"
            );
        }
    }

    /// With `base = [0, 0]` the new formula is byte-identical to the legacy one. We recompute the
    /// legacy result inline (the exact code that lived in `tournament_icm_values_for_terminal`
    /// before this change) and assert equality across all winners and several contributions.
    #[test]
    fn base_zero_is_byte_identical_to_legacy() {
        let mut game = minimal_game();
        let config = icm_config();

        let legacy = |starting_pot: i32, contribution: [i32; 2], winner: Option<usize>| {
            let mut stacks = config.stacks.clone();
            let starting_pot = starting_pot as f64;
            let contribution = [contribution[0] as f64, contribution[1] as f64];
            let pot = starting_pot + contribution[0] + contribution[1];
            match winner {
                Some(0) => {
                    stacks[config.oop_seat] = stacks[config.oop_seat] - contribution[0] + pot;
                    stacks[config.ip_seat] -= contribution[1];
                }
                Some(1) => {
                    stacks[config.ip_seat] = stacks[config.ip_seat] - contribution[1] + pot;
                    stacks[config.oop_seat] -= contribution[0];
                }
                Some(_) => unreachable!(),
                None => {
                    stacks[config.oop_seat] = stacks[config.oop_seat] - contribution[0] + pot * 0.5;
                    stacks[config.ip_seat] = stacks[config.ip_seat] - contribution[1] + pot * 0.5;
                }
            }
            terminal_icm_values(&stacks, &config.payouts).unwrap()
        };

        for &starting_pot in &[60, 100, 250] {
            for &contribution in &[[0, 0], [50, 50], [120, 90], [0, 70]] {
                game.tree_config.starting_pot = starting_pot;
                for winner in [Some(0usize), Some(1usize), None] {
                    let new = game
                        .tournament_icm_values_for_terminal(&config, contribution, [0, 0], winner)
                        .unwrap();
                    let old = legacy(starting_pot, contribution, winner);
                    assert_eq!(new, old, "starting_pot {starting_pot} contribution {contribution:?} winner {winner:?}");
                }
            }
        }
    }

    /// The :837 guard must trip exactly when a seat cannot cover its FULL investment
    /// (`base + contribution`), and must NOT trip when both seats can cover it. We exercise the
    /// guard through `precompute_terminal_icm_utilities`, building a tiny single-action tree so a
    /// terminal node exists, and choosing a `base_contribution` that pushes one seat over its stack.
    #[test]
    fn guard_trips_when_seat_cannot_cover_base_plus_contribution() {
        // effective_stack small so the river-local contribution stays tiny; the base pushes the
        // total over the seat's absolute stack.
        let card_config = CardConfig {
            range: [Range::ones(); 2],
            flop: flop_from_str("Td9d6h").unwrap(),
            ..Default::default()
        };
        let tree_config = TreeConfig {
            starting_pot: 60,
            effective_stack: 100,
            ..Default::default()
        };
        let action_tree = ActionTree::new(tree_config).unwrap();
        let game = PostFlopGame::with_config(card_config, action_tree).unwrap();

        // OOP seat 0 has only 30 chips. With base 25 the all-check terminal (river contribution 0)
        // already needs 25 <= 30 (OK). Bump base above the stack to force the guard.
        let config = TournamentIcmConfig {
            stacks: vec![30.0, 1000.0],
            payouts: vec![70, 30],
            oop_seat: 0,
            ip_seat: 1,
        };
        let baseline =
            game.tournament_icm_values_for_terminal(&config, [0, 0], [0, 0], None).unwrap();
        let baseline_oop = baseline[config.oop_seat];
        let baseline_ip = baseline[config.ip_seat];

        // base = [0, 0] -> contributions are 0 at the all-check terminal -> guard passes.
        assert!(game
            .precompute_terminal_icm_utilities(&config, [0, 0], baseline_oop, baseline_ip)
            .is_ok());

        // base = [40, 0] for OOP exceeds its 30-chip stack -> guard must trip.
        let err = game
            .precompute_terminal_icm_utilities(&config, [40, 0], baseline_oop, baseline_ip)
            .unwrap_err();
        assert!(
            err.contains("cannot cover terminal contribution 40"),
            "unexpected guard error: {err}"
        );
    }

    /// Installs an ICM config + base onto the game by hand so we can call the runtime
    /// `tournament_icm_fold_equivalent_delta` path directly. This mirrors exactly what
    /// `set_tournament_icm_config_inner` stores (`tournament_icm_config` + `base_contribution`),
    /// without requiring a solved tree — the delta only reads `tree_config.starting_pot`, the
    /// stored config, and the stored base.
    fn install_icm(game: &mut PostFlopGame, config: TournamentIcmConfig, base: [i32; 2]) {
        game.tournament_icm_config = Some(config);
        game.base_contribution = base;
    }

    /// PHYSICAL-EQUIVALENCE for the fold-equivalent centering: the SAME physical hand expressed two
    /// ways must yield the SAME `tournament_icm_fold_equivalent_delta` for each player.
    ///   FULL:   starting_pot = parent_pot,      config base [0, 0], delta(player, [P+t0, P+t1])
    ///   REROOT: starting_pot = parent_pot + 2P, config base [P, P], delta(player, [t0, t1])
    /// Because the delta is computed live from the STORED base, the re-rooted game must reconstruct
    /// the true full-hand fold-equivalent delta. A 3-seat payout ladder makes ICM equity nonlinear
    /// in the stack vector, so this would NOT hold if the base were dropped (see negative control).
    #[test]
    fn fold_equivalent_delta_reroot_matches_full_hand() {
        let parent_pot: i32 = 40;
        let p: i32 = 25; // pre-river investment per seat
        let t0: i32 = 60; // river-local investment, OOP
        let t1: i32 = 80; // river-local investment, IP

        for player in [0usize, 1usize] {
            // FULL framing.
            let mut full_game = minimal_game();
            full_game.tree_config.starting_pot = parent_pot;
            install_icm(&mut full_game, icm_config(), [0, 0]);
            let full = full_game
                .tournament_icm_fold_equivalent_delta(player, [p + t0, p + t1])
                .unwrap();

            // REROOT framing: starting_pot folds in the pre-river pot (2P), and the STORED base
            // [P, P] carries the pre-river investment.
            let mut reroot_game = minimal_game();
            reroot_game.tree_config.starting_pot = parent_pot + 2 * p;
            install_icm(&mut reroot_game, icm_config(), [p, p]);
            let reroot = reroot_game
                .tournament_icm_fold_equivalent_delta(player, [t0, t1])
                .unwrap();

            assert!(
                (full - reroot).abs() < 1e-5,
                "player {player}: FULL delta {full} != REROOT delta {reroot}"
            );
        }
    }

    /// Negative control for the fold-equivalent path: a re-rooted game with a STORED base of
    /// [0, 0] (what the buggy hardcoded `[0, 0]` produced) must give a DIFFERENT delta from the
    /// FULL framing. This proves the stored base is load-bearing for the REPORTED EVs (the
    /// fold-equivalent centering), not just for the terminal utilities.
    #[test]
    fn fold_equivalent_delta_reroot_without_base_differs_from_full() {
        let parent_pot: i32 = 40;
        let p: i32 = 25;
        let t0: i32 = 60;
        let t1: i32 = 80;

        for player in [0usize, 1usize] {
            let mut full_game = minimal_game();
            full_game.tree_config.starting_pot = parent_pot;
            install_icm(&mut full_game, icm_config(), [0, 0]);
            let full = full_game
                .tournament_icm_fold_equivalent_delta(player, [p + t0, p + t1])
                .unwrap();

            // Re-rooted starting_pot but base dropped to [0, 0]: the bug.
            let mut buggy_game = minimal_game();
            buggy_game.tree_config.starting_pot = parent_pot + 2 * p;
            install_icm(&mut buggy_game, icm_config(), [0, 0]);
            let buggy = buggy_game
                .tournament_icm_fold_equivalent_delta(player, [t0, t1])
                .unwrap();

            assert!(
                (full - buggy).abs() > 1e-3,
                "player {player}: base was cosmetic, FULL delta {full} == BUGGY delta {buggy}"
            );
        }
    }
}

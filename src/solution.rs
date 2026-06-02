//! Ergonomic, non-breaking wrappers over the low-level save/load primitives.
//!
//! `postflop-solver` already supports a *compact navigable save*: solve a game,
//! persist only the average strategy, and recompute the counterfactual values when
//! the file is loaded. This module packages that capability behind a small options
//! API so callers do not have to combine [`set_target_storage_mode`] and
//! [`save_data_to_file`] by hand.
//!
//! The wrappers add no new serialization format — a file written by [`save_solution`]
//! is byte-identical to one written by [`save_data_to_file`] with the same arguments,
//! and is readable by [`load_data_from_file`] (and vice versa).
//!
//! See `docs/compact-solution.md` for the on-disk format and the post-load capability
//! matrix (what each storage mode keeps, drops, and recomputes).
//!
//! # Internal precision is not a save-time option
//!
//! Whether node values are stored as 32-bit floats or 16-bit integers is decided by
//! [`PostFlopGame::allocate_memory`] (`enable_compression`) and *cannot* be changed at
//! save time — the in-memory buffers already have that representation. [`SaveOptions`]
//! therefore exposes only what is actually controllable when serializing: the storage
//! mode, the external zstd level, and the memo.
//!
//! # River mode is not safe for tournament-ICM solves
//!
//! A game solved with `set_tournament_icm_config` keeps its precomputed terminal ICM
//! utilities in memory, and those are *not* persisted. River mode recomputes
//! counterfactual values on load, and that recompute falls back to chip / bubble-factor EV,
//! yielding *incorrect* values for an ICM solve. Use [`BoardState::Turn`] / [`BoardState::Flop`]
//! for tournament-ICM solutions (those load the as-solved values without recomputing).
//! chipEV and `bubble_factor` solves are unaffected. See `docs/compact-solution.md`.
//!
//! [`set_target_storage_mode`]: PostFlopGame::set_target_storage_mode
//! [`PostFlopGame::allocate_memory`]: PostFlopGame::allocate_memory

use crate::{load_data_from_file, save_data_to_file, BoardState, PostFlopGame};
use std::path::Path;

/// Options controlling how a solved [`PostFlopGame`] is serialized.
///
/// Only knobs that are meaningful at save time are exposed; internal numeric precision
/// is fixed at allocation time (see the [module docs](self)).
#[derive(Debug, Clone)]
pub struct SaveOptions {
    /// Deepest board street to persist.
    ///
    /// - [`BoardState::River`] keeps the whole tree navigable. Only the strategy is
    ///   written to disk; counterfactual values are recomputed on load.
    /// - [`BoardState::Turn`] / [`BoardState::Flop`] physically drop the deeper nodes
    ///   from the file, so they cannot be navigated after loading.
    pub storage_mode: BoardState,

    /// zstd compression level (1-22). `None` disables compression.
    ///
    /// `Some(_)` requires the `zstd` feature; without it, saving returns an error
    /// (the same behavior as [`save_data_to_file`]).
    pub compression: Option<i32>,

    /// Free-form memo persisted in the file header and returned by [`load_solution`].
    pub memo: String,
}

impl SaveOptions {
    /// River storage mode, no compression.
    ///
    /// Always available (does not require the `zstd` feature). The whole tree stays
    /// navigable after loading.
    #[inline]
    pub fn navigable() -> Self {
        Self {
            storage_mode: BoardState::River,
            compression: None,
            memo: String::new(),
        }
    }

    /// River storage mode compressed with zstd at `level` (1-22).
    ///
    /// Produces the smallest fully navigable artifact. Requires the `zstd` feature at
    /// save time: without it, [`save_solution`] returns an error *after* truncating the
    /// destination file (inherited from [`save_data_to_file`]). Use [`Self::navigable`]
    /// when the feature may be absent.
    #[inline]
    pub fn compact(level: i32) -> Self {
        Self {
            storage_mode: BoardState::River,
            compression: Some(level),
            memo: String::new(),
        }
    }

    /// Sets the memo persisted in the file header.
    #[inline]
    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = memo.into();
        self
    }

    /// Sets the deepest board street to persist.
    #[inline]
    pub fn with_storage_mode(mut self, storage_mode: BoardState) -> Self {
        self.storage_mode = storage_mode;
        self
    }

    /// Sets the zstd compression level (`None` disables compression).
    #[inline]
    pub fn with_compression(mut self, compression: Option<i32>) -> Self {
        self.compression = compression;
        self
    }
}

impl Default for SaveOptions {
    /// Equivalent to [`SaveOptions::navigable`]: River mode, no compression. Chosen so
    /// that `Default` never fails when the `zstd` feature is disabled.
    fn default() -> Self {
        Self::navigable()
    }
}

/// Options controlling how a solution is loaded.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// Optional cap (in bytes) on the estimated memory usage of the loaded game.
    /// Loading fails if the estimate exceeds this value. `None` means no limit.
    pub max_memory_usage: Option<u64>,
}

impl LoadOptions {
    /// Sets the maximum estimated memory usage allowed when loading.
    #[inline]
    pub fn with_max_memory_usage(mut self, max_memory_usage: Option<u64>) -> Self {
        self.max_memory_usage = max_memory_usage;
        self
    }
}

/// Saves a solved [`PostFlopGame`] to `path` using ergonomic options.
///
/// This is a thin wrapper that applies `options.storage_mode` via
/// [`PostFlopGame::set_target_storage_mode`] and then delegates to
/// [`save_data_to_file`].
///
/// The game must be solved; otherwise the underlying `"Data is not ready to save"`
/// error is returned (the same gate as [`save_data_to_file`]).
///
/// For tournament-ICM solves, do not use River mode — its on-load recompute drops the
/// (unpersisted) ICM utilities. See the [module docs](self).
///
/// # Side effect
///
/// On success the game's target storage mode is left set to `options.storage_mode`.
/// This only affects subsequent serialization; it does not free in-memory buffers, so
/// the game remains fully queryable.
///
/// # Errors
///
/// Returns an error if the storage mode is out of range (deeper than the allocated
/// storage, or shallower than the tree's initial state), if the game is not solved, or
/// if compression is requested without the `zstd` feature.
pub fn save_solution<P: AsRef<Path>>(
    game: &mut PostFlopGame,
    path: P,
    options: &SaveOptions,
) -> Result<(), String> {
    game.set_target_storage_mode(options.storage_mode)?;
    save_data_to_file(game, &options.memo, path, options.compression)
}

/// Loads a solution previously saved by [`save_solution`] (or any compatible file).
///
/// Returns the game together with its memo string. For River-mode files this triggers
/// the on-load recompute of counterfactual values internally; see the post-load
/// capability matrix in `docs/compact-solution.md`.
pub fn load_solution<P: AsRef<Path>>(
    path: P,
    options: &LoadOptions,
) -> Result<(PostFlopGame, String), String> {
    load_data_from_file::<PostFlopGame, _>(path, options.max_memory_usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_tree::*;
    use crate::card::*;
    use crate::game::*;
    use crate::range::*;
    use crate::utility::*;

    /// Builds the tiny all-ones flop game used by the file-IO tests, already solved.
    fn solved_ones_flop_game() -> PostFlopGame {
        let card_config = CardConfig {
            range: [Range::ones(); 2],
            flop: flop_from_str("Td9d6h").unwrap(),
            ..Default::default()
        };
        let tree_config = TreeConfig {
            starting_pot: 60,
            effective_stack: 970,
            flop_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
            turn_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
            ..Default::default()
        };
        let action_tree = ActionTree::new(tree_config).unwrap();
        let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
        game.allocate_memory(false);
        finalize(&mut game);
        game
    }

    fn assert_balanced_root(game: &mut PostFlopGame) {
        game.cache_normalized_weights();
        let weights_oop = game.normalized_weights(0);
        let weights_ip = game.normalized_weights(1);
        let root_equity_oop = compute_average(&game.equity(0), weights_oop);
        let root_equity_ip = compute_average(&game.equity(1), weights_ip);
        let root_ev_oop = compute_average(&game.expected_values(0), weights_oop);
        let root_ev_ip = compute_average(&game.expected_values(1), weights_ip);

        assert!((root_equity_oop - 0.5).abs() < 1e-5);
        assert!((root_equity_ip - 0.5).abs() < 1e-5);
        assert!((root_ev_oop - 45.0).abs() < 1e-4);
        assert!((root_ev_ip - 15.0).abs() < 1e-4);
    }

    #[test]
    fn default_save_options_is_navigable() {
        let options = SaveOptions::default();
        assert_eq!(options.storage_mode, BoardState::River);
        assert!(options.compression.is_none());
        assert!(options.memo.is_empty());
    }

    #[test]
    fn save_solution_river_roundtrip_uncompressed() {
        let mut game = solved_ones_flop_game();
        let path = std::env::temp_dir().join(format!(
            "pfs_compact_river_uncompressed_{}.bin",
            std::process::id()
        ));

        save_solution(&mut game, &path, &SaveOptions::navigable().with_memo("memo")).unwrap();
        let (mut loaded, memo) = load_solution(&path, &LoadOptions::default()).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(memo, "memo");
        assert_balanced_root(&mut loaded);
    }

    #[test]
    #[cfg(feature = "zstd")]
    fn save_solution_river_roundtrip_compressed() {
        let mut game = solved_ones_flop_game();
        let path = std::env::temp_dir().join(format!(
            "pfs_compact_river_compressed_{}.bin",
            std::process::id()
        ));

        save_solution(&mut game, &path, &SaveOptions::compact(3)).unwrap();
        let (mut loaded, _memo) = load_solution(&path, &LoadOptions::default()).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_balanced_root(&mut loaded);
    }

    #[test]
    fn save_solution_rejects_mode_below_initial_state() {
        // A turn-initial game cannot be saved as `Flop`: the bound check in
        // `set_target_storage_mode` rejects it before any file is created.
        let card_config = CardConfig {
            range: [Range::ones(); 2],
            flop: flop_from_str("Td9d6h").unwrap(),
            turn: card_from_str("Qc").unwrap(),
            ..Default::default()
        };
        let tree_config = TreeConfig {
            initial_state: BoardState::Turn,
            starting_pot: 60,
            effective_stack: 970,
            turn_bet_sizes: [("50%", "").try_into().unwrap(), Default::default()],
            ..Default::default()
        };
        let action_tree = ActionTree::new(tree_config).unwrap();
        let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();
        game.allocate_memory(false);

        let options = SaveOptions::navigable().with_storage_mode(BoardState::Flop);
        let err = save_solution(&mut game, "unused.bin", &options).unwrap_err();
        assert!(
            err.contains("lower value than the initial state"),
            "unexpected error: {err}"
        );
    }
}

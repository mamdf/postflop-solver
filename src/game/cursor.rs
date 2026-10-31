use super::interpreter::zero_dead_weights;
use super::*;
use crate::interface::*;
use crate::sliceop::*;
use crate::utility::*;

/// A read-only cursor over a [`PostFlopGame`]: a node, its history and both players' reach
/// weights, moved without touching the game's current node, so many cursors can walk one game at
/// once (e.g. one per thread). [`play`] returns the child and leaves `self` where it was.
///
/// Two limits keep it equal to the game's own interpreter, i.e. [`strategy`] and [`weights`] at a
/// history are the bits [`PostFlopGame::strategy`] and [`PostFlopGame::weights`] give after
/// [`PostFlopGame::apply_history`] of it: no bunching effect, and a chance step must be a card
/// listed by [`available_actions`] (no isomorphic suit swap).
///
/// [`play`]: #method.play
/// [`strategy`]: #method.strategy
/// [`weights`]: #method.weights
/// [`available_actions`]: #method.available_actions
#[derive(Clone)]
pub struct NodeCursor<'a> {
    game: &'a PostFlopGame,
    index: usize,
    history: Vec<usize>,
    turn: Card,
    river: Card,
    weights: [Vec<f32>; 2],
}

impl PostFlopGame {
    /// A [`NodeCursor`] at the root node.
    ///
    /// Panics if the memory is not yet allocated or the bunching effect is enabled.
    pub fn root_cursor(&self) -> NodeCursor<'_> {
        if self.state < State::MemoryAllocated {
            panic!("Memory is not allocated");
        }

        if self.bunching_num_dead_cards != 0 {
            panic!("NodeCursor does not support the bunching effect");
        }

        let (turn, river) = (self.card_config.turn, self.card_config.river);
        let mut weights = self.initial_weights.clone();
        zero_dead_weights(&self.private_cards, turn, river, &mut weights);
        NodeCursor {
            game: self,
            index: 0,
            history: Vec::new(),
            turn,
            river,
            weights,
        }
    }
}

impl<'a> NodeCursor<'a> {
    #[inline]
    fn node(&self) -> MutexGuardLike<'a, PostFlopNode> {
        self.game.node_arena[self.index].lock()
    }

    /// See [`PostFlopGame::history`].
    #[inline]
    pub fn history(&self) -> &[usize] {
        &self.history
    }

    /// See [`PostFlopGame::is_terminal_node`].
    #[inline]
    pub fn is_terminal_node(&self) -> bool {
        let node = self.node();
        node.is_terminal() || node.amount == self.game.tree_config.effective_stack
    }

    /// See [`PostFlopGame::is_chance_node`].
    #[inline]
    pub fn is_chance_node(&self) -> bool {
        self.node().is_chance() && !self.is_terminal_node()
    }

    /// See [`PostFlopGame::available_actions`].
    pub fn available_actions(&self) -> Vec<Action> {
        if self.is_terminal_node() {
            Vec::new()
        } else {
            self.node()
                .children()
                .iter()
                .map(|c| c.lock().prev_action)
                .collect()
        }
    }

    /// See [`PostFlopGame::current_player`].
    #[inline]
    pub fn current_player(&self) -> usize {
        if self.is_terminal_node() {
            panic!("Terminal node is not allowed");
        }

        if self.is_chance_node() {
            panic!("Chance node is not allowed");
        }

        self.node().player()
    }

    /// See [`PostFlopGame::weights`].
    #[inline]
    pub fn weights(&self, player: usize) -> &[f32] {
        &self.weights[player]
    }

    /// See [`PostFlopGame::strategy`].
    pub fn strategy(&self) -> Vec<f32> {
        let player = self.current_player();
        let node = self.node();
        let num_actions = node.num_actions();

        let mut ret = if self.game.is_compression_enabled {
            normalized_strategy_compressed(node.strategy_compressed(), num_actions)
        } else {
            normalized_strategy(node.strategy(), num_actions)
        };

        apply_locking_strategy(&mut ret, self.game.locking_strategy(&node));
        debug_assert_eq!(ret.len(), num_actions * self.game.num_private_hands(player));
        ret
    }

    /// The cursor after `action`, as [`PostFlopGame::play`] would move the game: an action index
    /// at a player node, a card ID listed by [`available_actions`] at a chance node.
    ///
    /// Panics if the current node is terminal or `action` is not one of those.
    ///
    /// [`available_actions`]: #method.available_actions
    pub fn play(&self, action: usize) -> NodeCursor<'a> {
        if self.is_terminal_node() {
            panic!("Terminal node is not allowed");
        }

        let mut child = self.clone();
        let node = self.node();
        if self.is_chance_node() {
            let card = action as Card;
            let index = node
                .children()
                .iter()
                .position(|c| c.lock().prev_action == Action::Chance(card))
                .expect("Invalid action: not a card listed by available_actions");
            if self.turn == NOT_DEALT {
                child.turn = card;
            } else {
                child.river = card;
            }
            zero_dead_weights(
                &self.game.private_cards,
                child.turn,
                child.river,
                &mut child.weights,
            );
            child.index = self.game.node_index(&node.play(index));
        } else {
            if action >= node.num_actions() {
                panic!("Invalid action");
            }
            let player = node.player();
            if node.num_actions() > 1 {
                let strategy = self.strategy();
                let num_hands = self.game.num_private_hands(player);
                mul_slice(
                    &mut child.weights[player],
                    row(&strategy, action, num_hands),
                );
            }
            child.index = self.game.node_index(&node.play(action));
        }
        child.history.push(action);
        child
    }
}

use crate::game::Action;
use crate::types::{PlayerId, Resource, Resources};
use serde::{Deserialize, Serialize};

/// Resources a single player received from a dice roll.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePayout {
    pub player: PlayerId,
    pub resources: Resources,
}

/// Net resource change for one player during an action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDelta {
    pub player: PlayerId,
    pub gained: Resources,
    pub lost: Resources,
}

/// How many cards a player must discard after a 7.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscardDemand {
    pub player: PlayerId,
    pub count: u16,
}

/// Longest road or largest army changing as a result of an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameAward {
    LongestRoad,
    LargestArmy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AwardChange {
    pub award: GameAward,
    pub previous: Option<PlayerId>,
    pub holder: Option<PlayerId>,
}

/// A non-deterministic outcome produced while handling an action.
/// These values are recorded so a replay can reproduce the exact game state
/// without needing to persist the RNG.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventOutcome {
    None,
    DiceRoll {
        d1: u8,
        d2: u8,
        #[serde(default)]
        production: Vec<ResourcePayout>,
        #[serde(default)]
        discards: Vec<DiscardDemand>,
    },
    ResourceStolen {
        resource: Resource,
    },
    ResourcesGained {
        resources: Resources,
    },
    Monopoly {
        resource: Resource,
        amount: u16,
    },
}

/// A single entry in a game's event log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameEvent {
    /// Monotonic index of this event within the log.
    pub sequence: usize,
    /// The player who performed the action.
    pub player: PlayerId,
    /// The action taken.
    pub action: Action,
    /// Any non-deterministic outcome associated with the action.
    pub outcome: EventOutcome,
    /// Per-player resource gains and losses caused by this action.
    #[serde(default)]
    pub resource_changes: Vec<ResourceDelta>,
    /// Longest road / largest army holder changes caused by this action.
    #[serde(default)]
    pub award_changes: Vec<AwardChange>,
    /// Set when this action caused a player to reach 10 victory points.
    #[serde(default)]
    pub winner: Option<PlayerId>,
    /// Unix time in milliseconds when the action was applied.
    #[serde(default)]
    pub timestamp_ms: u64,
}

impl GameEvent {
    /// Compare gameplay fields, ignoring wall-clock timestamps.
    pub fn same_gameplay(&self, other: &Self) -> bool {
        self.sequence == other.sequence
            && self.player == other.player
            && self.action == other.action
            && self.outcome == other.outcome
            && self.resource_changes == other.resource_changes
            && self.award_changes == other.award_changes
            && self.winner == other.winner
    }
}

/// A subscriber that receives every event as it is applied to a game.
///
/// Implement this trait to broadcast events to a file, message queue, audit
/// store, WebSocket channel, or any other sink. Subscribers are called
/// synchronously inside `Game::apply_action` after the action succeeds.
pub trait EventSubscriber: Send + Sync {
    fn on_event(&self, event: &GameEvent);
}

/// A complete, replayable log for one game.
///
/// The log stores the initial players and the random seed used to generate the
/// board, turn order, and development deck. Replaying the recorded events in
/// order reconstructs the exact game state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameLog {
    pub seed: u64,
    pub initial_players: Vec<crate::player::Player>,
    pub initial_player_order: Vec<PlayerId>,
    pub events: Vec<GameEvent>,
}

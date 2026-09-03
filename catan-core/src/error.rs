use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatanError {
    #[error("Not your turn")]
    NotYourTurn,
    #[error("Invalid action for current phase")]
    InvalidPhase,
    #[error("Invalid player")]
    InvalidPlayer,
    #[error("Game is already finished")]
    GameFinished,
    #[error("Invalid build location")]
    InvalidBuildLocation,
    #[error("Not enough resources")]
    NotEnoughResources,
    #[error("No pieces remaining")]
    NoPiecesRemaining,
    #[error("Must roll dice first")]
    MustRollDice,
    #[error("Robber must be moved")]
    MustMoveRobber,
    #[error("Required resources must be discarded first")]
    MustDiscard,
    #[error("Invalid robber location")]
    InvalidRobberLocation,
    #[error("Invalid robber target")]
    InvalidRobberTarget,
    #[error("Invalid trade")]
    InvalidTrade,
    #[error("No pending trade")]
    NoPendingTrade,
    #[error("Trade not for you")]
    TradeNotForYou,
    #[error("Cannot play a development card this turn")]
    CannotPlayDevCard,
    #[error("No development cards left")]
    NoDevCards,
    #[error("Road length calculation failed")]
    RoadLengthFailed,
    #[error("Unknown error: {0}")]
    Unknown(String),
}

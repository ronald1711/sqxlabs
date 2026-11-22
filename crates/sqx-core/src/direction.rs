use serde::{Deserialize, Serialize};

/// Trade direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Direction {
    #[default]
    Long,
    Short,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Direction::Long => Direction::Short,
            Direction::Short => Direction::Long,
        }
    }

    /// Sign applied to price moves: +1 for long, -1 for short.
    pub fn sign(self) -> f64 {
        match self {
            Direction::Long => 1.0,
            Direction::Short => -1.0,
        }
    }
}

/// The flat or net position currently held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PositionKind {
    #[default]
    Flat,
    Long,
    Short,
}

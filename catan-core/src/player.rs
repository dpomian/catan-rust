use crate::types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub color: PlayerColor,
    pub resources: Resources,
    pub hand: Vec<DevelopmentCard>,
    /// Cards bought this turn that cannot yet be played (except VP cards to win).
    pub new_cards: Vec<DevelopmentCard>,
    pub played_knights: u8,
    pub victory_point_cards: u8,
    pub remaining_roads: u8,
    pub remaining_settlements: u8,
    pub remaining_cities: u8,
}

impl Player {
    /// Create a player with the standard starting pieces and empty resources/cards.
    pub fn new(id: PlayerId, name: impl Into<String>, color: PlayerColor) -> Self {
        Self {
            id,
            name: name.into(),
            color,
            resources: Resources::zero(),
            hand: Vec::new(),
            new_cards: Vec::new(),
            played_knights: 0,
            victory_point_cards: 0,
            remaining_roads: 15,
            remaining_settlements: 5,
            remaining_cities: 4,
        }
    }

    /// Count all development cards, including those bought this turn.
    pub fn total_cards(&self) -> u8 {
        self.hand.len() as u8 + self.new_cards.len() as u8
    }

    /// Check whether a playable development card is in the active hand.
    pub fn has_card(&self, card: DevelopmentCard) -> bool {
        self.hand.contains(&card)
    }

    /// Remove a development card from the active hand.
    pub fn remove_card(&mut self, card: DevelopmentCard) -> bool {
        if let Some(pos) = self.hand.iter().position(|c| *c == card) {
            self.hand.remove(pos);
            true
        } else {
            false
        }
    }

    /// Count victory points visible to all players (revealed VP cards only).
    pub fn public_victory_points(&self) -> u8 {
        self.victory_point_cards
    }

    pub fn hidden_victory_points(&self) -> u8 {
        self.hand
            .iter()
            .chain(&self.new_cards)
            .filter(|&&card| card == DevelopmentCard::VictoryPoint)
            .count() as u8
    }

    /// Check whether this card can be played now (VP cards are always revealable).
    pub fn can_play_card(&self, card: DevelopmentCard) -> bool {
        match card {
            DevelopmentCard::VictoryPoint => true,
            _ => self.hand.contains(&card),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_player_has_expected_defaults() {
        let p = Player::new(2, "Alice".to_string(), PlayerColor::Blue);
        assert_eq!(p.id, 2);
        assert_eq!(p.name, "Alice");
        assert_eq!(p.color, PlayerColor::Blue);
        assert_eq!(p.resources, Resources::zero());
        assert!(p.hand.is_empty());
        assert!(p.new_cards.is_empty());
        assert_eq!(p.played_knights, 0);
        assert_eq!(p.victory_point_cards, 0);
        assert_eq!(p.remaining_roads, 15);
        assert_eq!(p.remaining_settlements, 5);
        assert_eq!(p.remaining_cities, 4);
    }

    #[test]
    fn total_cards_counts_both_piles() {
        let mut p = Player::new(0, "B".to_string(), PlayerColor::Red);
        assert_eq!(p.total_cards(), 0);
        p.hand.push(DevelopmentCard::Knight);
        p.hand.push(DevelopmentCard::Monopoly);
        p.new_cards.push(DevelopmentCard::VictoryPoint);
        assert_eq!(p.total_cards(), 3);
    }

    #[test]
    fn has_and_remove_card_only_looks_at_hand() {
        let mut p = Player::new(0, "C".to_string(), PlayerColor::White);
        p.hand.push(DevelopmentCard::Knight);
        p.new_cards.push(DevelopmentCard::Knight);
        assert!(p.has_card(DevelopmentCard::Knight));
        assert!(p.remove_card(DevelopmentCard::Knight));
        assert!(!p.has_card(DevelopmentCard::Knight));
        assert!(!p.remove_card(DevelopmentCard::Knight));
    }

    #[test]
    fn victory_point_card_is_always_playable() {
        let p = Player::new(0, "D".to_string(), PlayerColor::Orange);
        assert!(p.can_play_card(DevelopmentCard::VictoryPoint));
        assert!(!p.can_play_card(DevelopmentCard::Knight));
    }

    #[test]
    fn public_victory_points_only_count_revealed_cards() {
        let mut p = Player::new(0, "E".to_string(), PlayerColor::Red);
        assert_eq!(p.public_victory_points(), 0);
        p.victory_point_cards = 2;
        assert_eq!(p.public_victory_points(), 2);
    }
}

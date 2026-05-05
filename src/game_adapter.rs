//! `napo_core` の `Match` を server 側の wire(`protocol`)につなぐアダプタ。
//!
//! - `Card` ↔ `"SA"` 文字列(A9 で確定)の変換
//! - `Action` ↔ `WireAction`、`Trump` ↔ `WireTrump` などの直訳
//! - `apply` 前後の `Phase` 差分から `PublicEvent` を組み立てる

#![allow(dead_code)] // Room actor (サブステップ C) が GameSession の API を順次叩く

use std::collections::HashMap;

use napo_core::card::Suit;
use napo_core::state::CompletedTrick;
use napo_core::trick::PlayedCard;
use napo_core::{Action, Bid, Card, Match, Observation, Phase, Rules, Trump};

use crate::protocol::{
    PlayerId, PublicEvent, SeatAssignment, WireAction, WireBid, WireBidEntry,
    WireCompletedTrick, WireCurrentBid, WireObservation, WirePlayedCard, WireTrump,
};

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("player is not in this game")]
    NotInGame,
    #[error("not your turn (current_player = {current:?})")]
    NotYourTurn { current: Option<usize> },
    #[error("invalid card '{0}'")]
    InvalidCard(String),
    #[error(transparent)]
    Core(#[from] napo_core::Error),
}

pub struct GameSession {
    session: Match,
    seat_to_player: Vec<PlayerId>,
    seat_display_names: Vec<String>,
    player_to_seat: HashMap<PlayerId, usize>,
    hands_per_match: u32,
    last_phase: Phase,
}

pub struct ApplyOutcome {
    pub events: Vec<PublicEvent>,
    pub observations: HashMap<PlayerId, WireObservation>,
    pub hand_finished: bool,
    pub match_finished: bool,
    pub hand_scores: Option<Vec<i32>>,
    pub cumulative_scores: Vec<i32>,
}

impl GameSession {
    /// `seated[i]` が seat `i` に座る。長さ = 5 必須。
    pub fn new(seated: Vec<(PlayerId, String)>, seed: u64, hands_per_match: u32) -> Self {
        assert_eq!(seated.len(), 5, "expected 5 seated players");
        let session = Match::new(Rules::default(), seed);
        let last_phase = session.current_hand().phase().clone();
        let seat_to_player: Vec<PlayerId> = seated.iter().map(|(p, _)| p.clone()).collect();
        let seat_display_names: Vec<String> = seated.into_iter().map(|(_, n)| n).collect();
        let player_to_seat: HashMap<_, _> = seat_to_player
            .iter()
            .enumerate()
            .map(|(seat, p)| (p.clone(), seat))
            .collect();
        Self {
            session,
            seat_to_player,
            seat_display_names,
            player_to_seat,
            hands_per_match,
            last_phase,
        }
    }

    pub fn seat_assignments(&self) -> Vec<SeatAssignment> {
        self.seat_to_player
            .iter()
            .enumerate()
            .map(|(seat, pid)| SeatAssignment {
                seat,
                player_id: pid.clone(),
                display_name: self.seat_display_names[seat].clone(),
            })
            .collect()
    }

    pub fn cumulative_scores(&self) -> Vec<i32> {
        self.session.cumulative_scores().to_vec()
    }

    pub fn observations_for_all(&self) -> HashMap<PlayerId, WireObservation> {
        self.player_to_seat
            .iter()
            .map(|(pid, &seat)| {
                (
                    pid.clone(),
                    to_wire_observation(&self.session.current_hand().observation(seat)),
                )
            })
            .collect()
    }

    pub fn current_player(&self) -> Option<usize> {
        self.session.current_hand().current_player()
    }

    pub fn is_hand_finished(&self) -> bool {
        self.session.current_hand().is_finished()
    }

    pub fn hand_index(&self) -> u32 {
        self.session.hand_count() as u32
    }

    pub fn apply(
        &mut self,
        player: &PlayerId,
        wire: WireAction,
    ) -> Result<ApplyOutcome, AdapterError> {
        let seat = *self.player_to_seat.get(player).ok_or(AdapterError::NotInGame)?;
        let current = self.session.current_hand().current_player();
        if current != Some(seat) {
            return Err(AdapterError::NotYourTurn { current });
        }
        let action = wire_action_to_core(&wire)?;
        let before = self.session.current_hand().phase().clone();
        self.session.apply(action.clone())?;
        let after = self.session.current_hand().phase().clone();

        let events = self.derive_events(seat, &action, &before, &after);
        let observations = self.observations_for_all();
        let hand_finished = self.session.current_hand().is_finished();
        let hand_scores = if hand_finished {
            Some(self.session.current_hand().scores().unwrap().to_vec())
        } else {
            None
        };
        // hand_count は start_next_hand 後に増えるので、現局を含むには +1
        let match_finished =
            hand_finished && (self.session.hand_count() as u32 + 1) >= self.hands_per_match;

        self.last_phase = after;
        Ok(ApplyOutcome {
            events,
            observations,
            hand_finished,
            match_finished,
            hand_scores,
            cumulative_scores: self.cumulative_scores(),
        })
    }

    pub fn start_next_hand(&mut self) -> Result<ApplyOutcome, AdapterError> {
        self.session.start_next_hand()?;
        let new_phase = self.session.current_hand().phase().clone();
        let first_bidder = match &new_phase {
            Phase::Bidding { next, .. } => *next,
            _ => unreachable!("new hand must start in Bidding phase"),
        };
        self.last_phase = new_phase;
        Ok(ApplyOutcome {
            events: vec![PublicEvent::HandStarted {
                hand_index: self.session.hand_count() as u32,
                first_bidder,
            }],
            observations: self.observations_for_all(),
            hand_finished: false,
            match_finished: false,
            hand_scores: None,
            cumulative_scores: self.cumulative_scores(),
        })
    }

    fn derive_events(
        &self,
        seat: usize,
        action: &Action,
        before: &Phase,
        after: &Phase,
    ) -> Vec<PublicEvent> {
        let mut events = Vec::new();

        // Action 起源イベント
        match action {
            Action::Bid(b) => events.push(PublicEvent::Bidded {
                seat,
                bid: bid_to_wire(b),
            }),
            Action::Pass => events.push(PublicEvent::Passed { seat }),
            Action::DeclareAdjutant(card) => events.push(PublicEvent::AdjutantDeclared {
                card: card_to_wire(*card),
            }),
            Action::ExchangeWidow { discard } => {
                let revealed: Vec<String> = discard
                    .iter()
                    .copied()
                    .filter(|c| c.is_face())
                    .map(card_to_wire)
                    .collect();
                events.push(PublicEvent::WidowExchanged {
                    revealed_face_cards: revealed,
                });
            }
            Action::PlayCard(card) => events.push(PublicEvent::CardPlayed {
                seat,
                card: card_to_wire(*card),
            }),
        }

        // フェーズ遷移由来イベント
        match (before, after) {
            (Phase::Bidding { .. }, Phase::DeclareAdjutant { declarer, bid }) => {
                events.push(PublicEvent::NapoleonDecided {
                    seat: *declarer,
                    bid: bid_to_wire(bid),
                });
            }
            (
                Phase::Playing {
                    completed: cb,
                    adjutant_player: ab,
                    ..
                },
                Phase::Playing {
                    completed: ca,
                    adjutant_player: aa,
                    next: na,
                    ..
                },
            ) => {
                if ab.is_none()
                    && let Some(adj_seat) = aa
                {
                    events.push(PublicEvent::AdjutantRevealed { seat: *adj_seat });
                }
                if ca.len() > cb.len() {
                    events.push(PublicEvent::TrickWon { seat: *na });
                }
            }
            (
                Phase::Playing {
                    adjutant_player: ab,
                    completed: cb,
                    ..
                },
                Phase::Finished {
                    adjutant_player: aa,
                    scores,
                    tricks,
                    ..
                },
            ) => {
                if ab.is_none()
                    && let Some(adj_seat) = aa
                {
                    events.push(PublicEvent::AdjutantRevealed { seat: *adj_seat });
                }
                // 最終トリックの勝者
                if tricks.len() > cb.len()
                    && let Some(last) = tricks.last()
                {
                    events.push(PublicEvent::TrickWon { seat: last.winner });
                }
                events.push(PublicEvent::HandEnded {
                    scores: scores.clone(),
                });
            }
            _ => {}
        }

        // 配り直し: 「Pass が累積していた Bidding」→「Pass が全部 false に戻った Bidding」
        if let (
            Phase::Bidding { passed: pb, .. },
            Phase::Bidding { passed: pa, .. },
        ) = (before, after)
            && pb.iter().any(|p| *p)
            && pa.iter().all(|p| !*p)
        {
            events.push(PublicEvent::HandRedealt);
        }

        events
    }
}

// --- 純粋な変換ヘルパ ---

pub fn card_to_wire(c: Card) -> String {
    let s = match c.suit {
        Suit::Spade => 'S',
        Suit::Heart => 'H',
        Suit::Diamond => 'D',
        Suit::Club => 'C',
    };
    format!("{}{}", s, c.rank)
}

pub fn wire_to_card(s: &str) -> Result<Card, AdapterError> {
    s.parse::<Card>()
        .map_err(|_| AdapterError::InvalidCard(s.to_string()))
}

pub fn bid_to_wire(b: &Bid) -> WireBid {
    WireBid {
        tricks: b.tricks,
        trump: trump_to_wire(b.trump),
    }
}

pub fn trump_to_wire(t: Trump) -> WireTrump {
    match t {
        Trump::Suit(Suit::Spade) => WireTrump::S,
        Trump::Suit(Suit::Heart) => WireTrump::H,
        Trump::Suit(Suit::Diamond) => WireTrump::D,
        Trump::Suit(Suit::Club) => WireTrump::C,
        Trump::NoTrump => WireTrump::NT,
    }
}

pub fn wire_to_trump(t: WireTrump) -> Trump {
    match t {
        WireTrump::S => Trump::Suit(Suit::Spade),
        WireTrump::H => Trump::Suit(Suit::Heart),
        WireTrump::D => Trump::Suit(Suit::Diamond),
        WireTrump::C => Trump::Suit(Suit::Club),
        WireTrump::NT => Trump::NoTrump,
    }
}

pub fn wire_action_to_core(w: &WireAction) -> Result<Action, AdapterError> {
    Ok(match w {
        WireAction::Bid { tricks, trump } => Action::Bid(Bid::new(*tricks, wire_to_trump(*trump))),
        WireAction::Pass => Action::Pass,
        WireAction::DeclareAdjutant { card } => Action::DeclareAdjutant(wire_to_card(card)?),
        WireAction::ExchangeWidow { discard } => Action::ExchangeWidow {
            discard: discard
                .iter()
                .map(|s| wire_to_card(s))
                .collect::<Result<_, _>>()?,
        },
        WireAction::PlayCard { card } => Action::PlayCard(wire_to_card(card)?),
    })
}

fn played_card_to_wire(p: &PlayedCard) -> WirePlayedCard {
    WirePlayedCard {
        seat: p.player,
        card: card_to_wire(p.card),
    }
}

fn completed_trick_to_wire(t: &CompletedTrick) -> WireCompletedTrick {
    WireCompletedTrick {
        plays: t.plays.iter().map(played_card_to_wire).collect(),
        winner: t.winner,
    }
}

pub fn to_wire_observation(o: &Observation) -> WireObservation {
    WireObservation {
        player: o.player,
        num_players: o.num_players,
        hand: o.hand.iter().copied().map(card_to_wire).collect(),
        bid_history: o
            .bid_history
            .iter()
            .map(|(seat, bid)| WireBidEntry {
                seat: *seat,
                bid: bid.as_ref().map(bid_to_wire),
            })
            .collect(),
        current_bid: o.current_bid.as_ref().map(|(seat, bid)| WireCurrentBid {
            seat: *seat,
            bid: bid_to_wire(bid),
        }),
        declarer: o.declarer,
        adjutant_card: o.adjutant_card.map(card_to_wire),
        adjutant_player: o.adjutant_player,
        completed: o.completed.iter().map(completed_trick_to_wire).collect(),
        current_trick: o.current_trick.iter().map(played_card_to_wire).collect(),
        trick_index: o.trick_index,
        to_act: o.to_act,
        visible_widow: o
            .visible_widow
            .as_ref()
            .map(|w| w.iter().copied().map(card_to_wire).collect()),
        revealed_face_discards: o
            .revealed_face_discards
            .iter()
            .copied()
            .map(card_to_wire)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use napo_core::card::{Rank, Suit};

    fn five_seated() -> Vec<(PlayerId, String)> {
        (0..5)
            .map(|i| (format!("p{i}"), format!("Player{i}")))
            .collect()
    }

    #[test]
    fn card_to_wire_uses_single_letter_suit_and_rank_string() {
        assert_eq!(card_to_wire(Card::new(Suit::Spade, Rank::Ace)), "SA");
        assert_eq!(card_to_wire(Card::new(Suit::Heart, Rank::Ten)), "H10");
        assert_eq!(card_to_wire(Card::new(Suit::Club, Rank::Two)), "C2");
        assert_eq!(card_to_wire(Card::new(Suit::Diamond, Rank::King)), "DK");
    }

    #[test]
    fn wire_to_card_roundtrip_full_deck() {
        for c in napo_core::deck::full_deck() {
            assert_eq!(wire_to_card(&card_to_wire(c)).unwrap(), c);
        }
    }

    #[test]
    fn trump_roundtrip() {
        for t in [
            Trump::Suit(Suit::Spade),
            Trump::Suit(Suit::Heart),
            Trump::Suit(Suit::Diamond),
            Trump::Suit(Suit::Club),
            Trump::NoTrump,
        ] {
            assert_eq!(wire_to_trump(trump_to_wire(t)), t);
        }
    }

    #[test]
    fn wire_action_bid_to_core() {
        let w = WireAction::Bid {
            tricks: 14,
            trump: WireTrump::H,
        };
        let a = wire_action_to_core(&w).unwrap();
        assert_eq!(a, Action::Bid(Bid::new(14, Trump::Suit(Suit::Heart))));
    }

    #[test]
    fn wire_action_invalid_card_errors() {
        let w = WireAction::PlayCard {
            card: "X9".into(),
        };
        assert!(matches!(
            wire_action_to_core(&w),
            Err(AdapterError::InvalidCard(_))
        ));
    }

    #[test]
    fn session_apply_emits_bidded_event() {
        let mut s = GameSession::new(five_seated(), 42, 10);
        let p0 = "p0".to_string();
        let out = s
            .apply(
                &p0,
                WireAction::Bid {
                    tricks: 13,
                    trump: WireTrump::S,
                },
            )
            .unwrap();
        assert!(
            out.events
                .iter()
                .any(|e| matches!(e, PublicEvent::Bidded { .. }))
        );
        assert!(!out.hand_finished);
        assert_eq!(out.observations.len(), 5);
    }

    #[test]
    fn session_apply_rejects_wrong_turn() {
        let mut s = GameSession::new(five_seated(), 42, 10);
        let p2 = "p2".to_string();
        let r = s.apply(
            &p2,
            WireAction::Bid {
                tricks: 13,
                trump: WireTrump::S,
            },
        );
        assert!(matches!(r, Err(AdapterError::NotYourTurn { current: Some(0) })));
    }

    #[test]
    fn session_apply_rejects_unknown_player() {
        let mut s = GameSession::new(five_seated(), 42, 10);
        let r = s.apply(&"ghost".to_string(), WireAction::Pass);
        assert!(matches!(r, Err(AdapterError::NotInGame)));
    }

    #[test]
    fn napoleon_decided_event_on_phase_transition() {
        let mut s = GameSession::new(five_seated(), 42, 10);
        // p0: Bid, p1..p4: Pass → DeclareAdjutant
        let _ = s
            .apply(
                &"p0".to_string(),
                WireAction::Bid {
                    tricks: 13,
                    trump: WireTrump::S,
                },
            )
            .unwrap();
        for i in 1..=3 {
            let _ = s.apply(&format!("p{i}"), WireAction::Pass).unwrap();
        }
        let out = s.apply(&"p4".to_string(), WireAction::Pass).unwrap();
        assert!(
            out.events
                .iter()
                .any(|e| matches!(e, PublicEvent::NapoleonDecided { seat: 0, .. }))
        );
    }

    #[test]
    fn redeal_event_when_all_pass() {
        let mut s = GameSession::new(five_seated(), 42, 10);
        for i in 0..=3 {
            let _ = s.apply(&format!("p{i}"), WireAction::Pass).unwrap();
        }
        let out = s.apply(&"p4".to_string(), WireAction::Pass).unwrap();
        assert!(
            out.events
                .iter()
                .any(|e| matches!(e, PublicEvent::HandRedealt))
        );
    }

    #[test]
    fn seat_assignments_match_input_order() {
        let s = GameSession::new(five_seated(), 42, 10);
        let assigns = s.seat_assignments();
        assert_eq!(assigns.len(), 5);
        for (i, a) in assigns.iter().enumerate() {
            assert_eq!(a.seat, i);
            assert_eq!(a.player_id, format!("p{i}"));
        }
    }
}

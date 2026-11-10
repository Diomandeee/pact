//! P9.1 — the two-reject stuck trigger, ported from the trajectory prototype.
//!
//! The finding, measured over 7,085 classified transitions: P(next user turn
//! is a rejection) is 15% at base, 49% after one rejection, 81% after two
//! consecutive rejections, and 10% after a reframe (below the shuffled null
//! of 14%, so a reframe actively suppresses the next reject rather than
//! merely resetting to base). The detector below is only the third leg of
//! that curve: it fires on the second of two consecutive rejections, and any
//! other move — reframe included — breaks the streak.
//!
//! Deliberately the trigger only. Text-to-move classification stays in Python
//! (`track.py`), behind a process boundary: the trigger is the one part that
//! must be fast and hot, while classification is I/O-bound on the model call
//! anyway. Parity with the Python oracle is pinned by
//! `tests/stuck_matches_oracle.rs`, not by sharing code.

#![forbid(unsafe_code)]

/// One classified user turn, as far as the trigger is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnMove {
    /// The user rejected or corrected the assistant's frame.
    Reject,
    /// The user reframed: scope change, definition change, restart.
    Reframe,
    /// Anything else: questions, constraints, acknowledgements, repeats.
    Other,
}

impl TurnMove {
    /// Map a prototype label (`REJECT_CORRECT`, `REFRAME`, …) to a trigger
    /// move. Unknown labels are [`TurnMove::Other`]: an unrecognised turn must
    /// break a streak, never extend one.
    #[must_use]
    pub fn from_label(label: &str) -> Self {
        match label {
            "REJECT_CORRECT" => Self::Reject,
            "REFRAME" => Self::Reframe,
            _ => Self::Other,
        }
    }
}

/// Fires on the second of two consecutive [`TurnMove::Reject`]s, and stays
/// lit for every further consecutive rejection while the streak holds.
///
/// Any other move resets the streak, which encodes the measured suppression:
/// after a reframe the next-turn reject rate falls below chance, so treating
/// a reframe as merely neutral would over-fire. Staying lit (rather than
/// firing once per streak) is the live-gate behaviour: the third consecutive
/// reject is more stuck, not less.
#[derive(Debug, Default)]
pub struct StuckDetector {
    consecutive_rejects: u32,
}

impl StuckDetector {
    /// A fresh detector with an empty streak.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one turn. Returns `true` on every turn the detector is firing.
    pub fn push(&mut self, turn: TurnMove) -> bool {
        if turn == TurnMove::Reject {
            self.consecutive_rejects += 1;
        } else {
            self.consecutive_rejects = 0;
        }
        self.consecutive_rejects >= 2
    }

    /// Feed a whole session, returning the indices that fire.
    #[must_use]
    pub fn feed(&mut self, turns: &[TurnMove]) -> Vec<usize> {
        turns
            .iter()
            .enumerate()
            .filter_map(|(i, &t)| self.push(t).then_some(i))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(labels: &[&str]) -> Vec<TurnMove> {
        labels.iter().map(|l| TurnMove::from_label(l)).collect()
    }

    #[test]
    fn fires_on_two_consecutive_rejects() {
        let mut d = StuckDetector::new();
        assert!(!d.push(TurnMove::Reject));
        assert!(d.push(TurnMove::Reject));
    }

    #[test]
    fn single_reject_is_silent() {
        let mut d = StuckDetector::new();
        assert!(!d.push(TurnMove::Reject));
        assert!(!d.push(TurnMove::Other));
        assert!(!d.push(TurnMove::Reject));
    }

    #[test]
    fn anything_between_breaks_the_streak() {
        // REJECT, QUESTION, REJECT must not fire: the 81% number counts
        // strictly consecutive pairs, and counting across an interruption
        // would inflate the trigger toward the 49% single-reject rate.
        let mut d = StuckDetector::new();
        let fired = d.feed(&seq(&["REJECT_CORRECT", "QUESTION", "REJECT_CORRECT"]));
        assert!(fired.is_empty(), "fired at {fired:?}");
    }

    #[test]
    fn reframe_breaks_the_streak() {
        // The suppression leg: REJECT, REFRAME, REJECT is a resolving
        // trajectory, and firing on its tail would punish the recovery.
        let mut d = StuckDetector::new();
        let fired = d.feed(&seq(&["REJECT_CORRECT", "REFRAME", "REJECT_CORRECT"]));
        assert!(fired.is_empty(), "fired at {fired:?}");
    }

    #[test]
    fn stays_lit_while_stuck() {
        let mut d = StuckDetector::new();
        let fired = d.feed(&seq(&[
            "REJECT_CORRECT",
            "REJECT_CORRECT",
            "REJECT_CORRECT",
        ]));
        assert_eq!(fired, vec![1, 2]);
    }

    #[test]
    fn fires_again_after_reset() {
        let mut d = StuckDetector::new();
        let fired = d.feed(&seq(&[
            "REJECT_CORRECT",
            "REJECT_CORRECT",
            "OTHER",
            "REJECT_CORRECT",
            "REJECT_CORRECT",
        ]));
        assert_eq!(fired, vec![1, 4]);
    }

    #[test]
    fn unknown_labels_are_other() {
        assert_eq!(TurnMove::from_label("REPEAT"), TurnMove::Other);
        assert_eq!(TurnMove::from_label(""), TurnMove::Other);
    }

    #[test]
    fn empty_and_single_are_silent() {
        let mut d = StuckDetector::new();
        assert!(d.feed(&[]).is_empty());
        assert!(d.feed(&seq(&["REJECT_CORRECT"])).is_empty());
    }
}

//! P9.2 — the redirect repertoire as a closed vocabulary.
//!
//! The taxonomy-instruct eval showed that handing the model the 7 maneuver
//! classes quadruples unprompted matches (8→32%). The repertoire is the
//! active ingredient, so it lives here as an exhaustive enum: the P8.2
//! reader — and the P9.3 restatement gate after it — can emit exactly these
//! 8 classes, and the type system makes a ninth bucket unrepresentable.
//!
//! The open world is handled honestly, not by widening the vocabulary:
//! [`Repertoire::classify`] returns `None` for unclassifiable input, and
//! `None` means "do not intervene", never a ninth maneuver. A live gate that
//! fires on unknown input would punish exactly the turns it understands
//! least.
//!
//! Parity for this module is two-sided. The fixture carries human-baked
//! `expected` classes; `tests/repertoire_matches_oracle.rs` checks the Rust
//! against the baked answers AND checks the baked answers against an
//! independent Python derivation from the rows' raw fields — so a
//! transcription slip in the fixture fails as loudly as a wrong formula.

#![forbid(unsafe_code)]

/// The 7 redirect maneuvers plus the explicit absence of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Maneuver {
    /// Abandon your own hypothesis: counter-evidence contradicts it.
    HypothesisKill,
    /// Redefine an infeasible task by changing the unit of work.
    ScaleReframe,
    /// Import evidence or method from outside the problem frame.
    DomainImport,
    /// Name a result invalid while preserving history; reroute correction.
    PremiseRejection,
    /// Paste prior-thread status to re-anchor on a reframed goal.
    ThreadTransplant,
    /// Stop the broken line; restart cleanly with new constraints.
    AbortRestart,
    /// A genuine redirect outside the six named classes.
    OtherManeuver,
    /// Continue the current line of work: no redirect, no reframe.
    Continuation,
}

impl Maneuver {
    /// All 8, in enum order. The closed vocabulary, enumerated so a test —
    /// not a comment — pins its size.
    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::HypothesisKill,
            Self::ScaleReframe,
            Self::DomainImport,
            Self::PremiseRejection,
            Self::ThreadTransplant,
            Self::AbortRestart,
            Self::OtherManeuver,
            Self::Continuation,
        ]
    }

    /// Map a prototype `maneuver_type` string to its class. Returns `None`
    /// for anything outside the vocabulary — including `"FALSE-POSITIVE"`,
    /// which names the absence of a redirect and is therefore the caller's
    /// decision (usually [`Maneuver::Continuation`]), not a ninth class.
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "hypothesis-kill-by-directional-evidence" => Some(Self::HypothesisKill),
            "scale-reframe" => Some(Self::ScaleReframe),
            "domain-structure-import" => Some(Self::DomainImport),
            "premise-rejection-with-immutability" => Some(Self::PremiseRejection),
            "thread-transplant" => Some(Self::ThreadTransplant),
            "abort-restart" => Some(Self::AbortRestart),
            "other-maneuver" => Some(Self::OtherManeuver),
            _ => None,
        }
    }

    /// Classify one labelled turn: a divergence verdict with a known
    /// maneuver class yields that class; anything else — `not` verdicts,
    /// false positives, missing labels — is [`Maneuver::Continuation`].
    /// Returns `None` only when the inputs contradict themselves (a
    /// divergence verdict with no known class), which is corrupt data, not
    /// a new maneuver.
    #[must_use]
    pub fn classify(verdict: &str, maneuver_label: Option<&str>) -> Option<Self> {
        match (verdict, maneuver_label) {
            ("divergence", Some(label)) => Self::from_label(label),
            ("divergence", None) => None,
            _ => Some(Self::Continuation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn vocabulary_is_exactly_eight() {
        let all = Maneuver::all();
        assert_eq!(all.len(), 8);
        let uniq: HashSet<_> = all.iter().collect();
        assert_eq!(uniq.len(), 8, "a duplicated variant is a ninth bucket");
    }

    #[test]
    fn all_seven_maneuvers_map() {
        let cases = [
            (
                "hypothesis-kill-by-directional-evidence",
                Maneuver::HypothesisKill,
            ),
            ("scale-reframe", Maneuver::ScaleReframe),
            ("domain-structure-import", Maneuver::DomainImport),
            (
                "premise-rejection-with-immutability",
                Maneuver::PremiseRejection,
            ),
            ("thread-transplant", Maneuver::ThreadTransplant),
            ("abort-restart", Maneuver::AbortRestart),
            ("other-maneuver", Maneuver::OtherManeuver),
        ];
        for (label, want) in cases {
            assert_eq!(Maneuver::from_label(label), Some(want), "{label}");
        }
    }

    #[test]
    fn outside_is_none_not_ninth() {
        for label in [
            "FALSE-POSITIVE",
            "",
            "continuation",
            "REJECT_CORRECT",
            "scope_change",
        ] {
            assert_eq!(Maneuver::from_label(label), None, "{label}");
        }
    }

    #[test]
    fn not_verdicts_continue() {
        // `not` means the turn performed no redirect — regardless of who
        // spoke or what the (absent) maneuver field says.
        assert_eq!(
            Maneuver::classify("not", None),
            Some(Maneuver::Continuation)
        );
        assert_eq!(
            Maneuver::classify("not", Some("FALSE-POSITIVE")),
            Some(Maneuver::Continuation)
        );
    }

    #[test]
    fn divergence_without_class_is_corrupt_not_new() {
        assert_eq!(Maneuver::classify("divergence", None), None);
        assert_eq!(Maneuver::classify("divergence", Some("nonsense")), None);
    }

    #[test]
    fn classify_agrees_with_from_label() {
        // The two entry points must never disagree about the same label:
        // one path for live turns and another for fixtures would be two
        // vocabularies sharing a name.
        for m in Maneuver::all() {
            if m == Maneuver::Continuation {
                continue;
            }
            let label = match m {
                Maneuver::HypothesisKill => "hypothesis-kill-by-directional-evidence",
                Maneuver::ScaleReframe => "scale-reframe",
                Maneuver::DomainImport => "domain-structure-import",
                Maneuver::PremiseRejection => "premise-rejection-with-immutability",
                Maneuver::ThreadTransplant => "thread-transplant",
                Maneuver::AbortRestart => "abort-restart",
                Maneuver::OtherManeuver => "other-maneuver",
                Maneuver::Continuation => unreachable!(),
            };
            assert_eq!(Maneuver::classify("divergence", Some(label)), Some(m));
        }
    }
}

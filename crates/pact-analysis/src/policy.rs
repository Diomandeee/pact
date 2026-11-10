//! P9.5 — the parametric restatement policy: measured unblock rates as
//! selection weights.
//!
//! Findings-0003 ranked maneuvers by outcome on 429 labeled rows. This
//! module turns those rates into a policy: given candidate reframe
//! classes, pick the one with the highest measured unblock rate. The
//! weights ARE the findings numbers — the gate pins each one, so a
//! re-measurement that changes the findings breaks this build until both
//! change together. Numbers that drift apart silently are how a policy
//! becomes folklore.
//!
//! Honest boundary, stated in code: weights exist only where measured.
//! Reframe classes with n<5 carry no weight (`None`) no matter how
//! suggestive a single row looks, and `Maneuver` variants (P9.2) are NOT
//! weighted at all — the join that could have rated them came back n=12,
//! noise. A policy that scores what it never measured is a horoscope.

#![forbid(unsafe_code)]

/// A measured unblock rate with its sample size. The `n` is load-bearing:
/// selection breaks near-ties toward more data, and classes below
/// [`MIN_N`] never enter the argmax.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate {
    /// Fraction unblocked, as measured.
    pub unblocked: f64,
    /// Rows behind the rate.
    pub n: u32,
}

/// Minimum rows for a class to be selectable. Below this the rate is
/// reported but never acted on: a single 1.000 (`analogy_import`,
/// `audience_shift`, `role_shift` all sit at n=1) would otherwise outrank
/// 145 rows at 0.931.
pub const MIN_N: u32 = 5;

/// Tieband: gaps smaller than this go to the larger sample, because a
/// 0.4pp lead on 9 rows is noise wearing a ranking.
pub const TIEBAND: f64 = 0.01;

// The two policy knobs are pinned at compile time: a change to either is a
// re-measurement and must move the findings in the same commit.
const _: () = assert!(MIN_N == 5);
const _: () = assert!(TIEBAND == 0.01);

impl Rate {
    const fn measured(unblocked: f64, n: u32) -> Self {
        Self { unblocked, n }
    }

    /// Selectable only with enough data behind it.
    #[must_use]
    pub const fn selectable(&self) -> bool {
        self.n >= MIN_N
    }
}

/// Measured rates by prototype reframe class (findings-0003, dataset-v3).
/// Each value is `Rate::measured(<fraction>, <n>)` with the exact n, so
/// the gate below checks both halves of every number.
pub mod reframe {
    use super::Rate;

    /// `scope_change`: 135/145 unblocked.
    pub const SCOPE_CHANGE: Rate = Rate::measured(0.931_034_482_758_620_7, 145);
    /// `definition_change`: 18/21 unblocked.
    pub const DEFINITION_CHANGE: Rate = Rate::measured(0.857_142_857_142_857_1, 21);
    /// `medium_change`: 7/9 unblocked.
    pub const MEDIUM_CHANGE: Rate = Rate::measured(0.777_777_777_777_777_8, 9);
    /// `constraint_add`: 6/13 unblocked — the maneuver that loses.
    pub const CONSTRAINT_ADD: Rate = Rate::measured(0.461_538_461_538_461_56, 13);

    /// Thin classes (n=1 each): reported, never selectable. Named here so
    /// a future re-measurement promotes them by editing one line each,
    /// not by redesigning the policy.
    /// `analogy_import`: 1/1, too thin to select.
    pub const ANALOGY_IMPORT: Rate = Rate::measured(1.0, 1);
    /// `audience_shift`: 1/1, too thin to select.
    pub const AUDIENCE_SHIFT: Rate = Rate::measured(1.0, 1);
    /// `role_shift`: 1/1, too thin to select.
    pub const ROLE_SHIFT: Rate = Rate::measured(1.0, 1);
    /// `constraint_remove`: 0/1, too thin to select.
    pub const CONSTRAINT_REMOVE: Rate = Rate::measured(0.0, 1);
}

/// Measured rates by turn move (findings-0003): `REFRAME` 160/173,
/// `REJECT_CORRECT` 154/230, Fisher p ≈ 2e-10.
pub mod turn {
    use super::Rate;

    /// `REFRAME`: 160/173 unblocked.
    pub const REFRAME: Rate = Rate::measured(0.924_855_491_329_479_8, 173);
    /// `REJECT_CORRECT`: 154/230 unblocked.
    pub const REJECT: Rate = Rate::measured(0.669_565_217_391_304_4, 230);
}

/// Pick the best candidate by expected unblock rate. Candidates are
/// `(name, Rate)` pairs; unselectable rates (`n < MIN_N`) are skipped,
/// near-ties (within [`TIEBAND`]) go to the larger sample, exact ties to
/// the earlier candidate. Returns `None` when nothing is selectable —
/// the caller falls back to repertoire order, not to a guess.
#[must_use]
pub fn select<'a>(candidates: &[(&'a str, Rate)]) -> Option<&'a str> {
    let mut best: Option<(&str, Rate)> = None;
    for &(name, rate) in candidates {
        if !rate.selectable() {
            continue;
        }
        match best {
            None => best = Some((name, rate)),
            Some((_, cur)) => {
                if rate.unblocked > cur.unblocked + TIEBAND
                    || ((rate.unblocked - cur.unblocked).abs() <= TIEBAND && rate.n > cur.n)
                {
                    best = Some((name, rate));
                }
            }
        }
    }
    best.map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::reframe::*;
    use super::turn::*;
    use super::{select, Rate};

    /// The gate that pins the policy to the findings: every weight, both
    /// halves. A re-measurement changes findings-0003 AND this test in one
    /// commit, or the build stays red until it does.
    #[test]
    fn weights_match_findings_0003() {
        assert_eq!(
            SCOPE_CHANGE,
            Rate {
                unblocked: 135.0 / 145.0,
                n: 145
            }
        );
        assert_eq!(
            DEFINITION_CHANGE,
            Rate {
                unblocked: 18.0 / 21.0,
                n: 21
            }
        );
        assert_eq!(
            MEDIUM_CHANGE,
            Rate {
                unblocked: 7.0 / 9.0,
                n: 9
            }
        );
        assert_eq!(
            CONSTRAINT_ADD,
            Rate {
                unblocked: 6.0 / 13.0,
                n: 13
            }
        );
        assert_eq!(
            REFRAME,
            Rate {
                unblocked: 160.0 / 173.0,
                n: 173
            }
        );
        assert_eq!(
            REJECT,
            Rate {
                unblocked: 154.0 / 230.0,
                n: 230
            }
        );
    }

    #[test]
    fn scope_first_constraint_last() {
        let all = [
            ("scope", SCOPE_CHANGE),
            ("definition", DEFINITION_CHANGE),
            ("medium", MEDIUM_CHANGE),
            ("constraint_add", CONSTRAINT_ADD),
        ];
        assert_eq!(select(&all), Some("scope"));
        // Constraint-adding alone still selects (it is measured), but any
        // alternative outranks it.
        assert_eq!(
            select(&[("constraint_add", CONSTRAINT_ADD)]),
            Some("constraint_add")
        );
        assert_eq!(
            select(&[
                ("constraint_add", CONSTRAINT_ADD),
                ("medium", MEDIUM_CHANGE)
            ]),
            Some("medium")
        );
    }

    #[test]
    fn thin_classes_never_select() {
        assert!(!ANALOGY_IMPORT.selectable());
        assert_eq!(
            select(&[("analogy", ANALOGY_IMPORT), ("scope", SCOPE_CHANGE)]),
            Some("scope")
        );
        // A 1.000 on one row must not outrank 145 rows at 0.931.
        assert_eq!(
            select(&[("scope", SCOPE_CHANGE), ("analogy", ANALOGY_IMPORT)]),
            Some("scope")
        );
        assert_eq!(select(&[("analogy", ANALOGY_IMPORT)]), None);
    }

    #[test]
    fn near_ties_go_to_more_data() {
        let thin_high = Rate {
            unblocked: 0.93,
            n: 9,
        };
        let fat_low = Rate {
            unblocked: 0.925,
            n: 145,
        };
        assert_eq!(
            select(&[("thin", thin_high), ("fat", fat_low)]),
            Some("fat")
        );
        // Outside the band, the rate wins regardless of n.
        let clear = Rate {
            unblocked: 0.80,
            n: 200,
        };
        assert_eq!(
            select(&[("thin", thin_high), ("clear", clear)]),
            Some("thin")
        );
    }

    #[test]
    fn reframe_outranks_reject() {
        assert_eq!(
            select(&[("reject", REJECT), ("reframe", REFRAME)]),
            Some("reframe")
        );
    }
}

//! Typed resources.
//!
//! The decision that shapes everything else: a resource is a TOKEN WITH A
//! TYPE, not a counter.
//!
//! An undifferentiated integer cannot say that `M_creative` has 33 producers
//! and 2 consumers while `M_plan` has 4 and 16. That imbalance is the single
//! most useful fact anyone has learned about the corpus, and it is per-type by
//! construction. A counter would have averaged it into nothing.
//!
//! It also makes a Converter carry a real signature rather than an analogy for
//! one: consumes a multiset of types, produces a multiset of types. That is
//! literally the SOOP-2 block on a skill, which means the corpus importer is a
//! translation rather than an interpretation.

use std::collections::BTreeMap;
use std::fmt;

/// The type of a resource: `M_plan`, `State`, `M_creative`.
///
/// A newtype rather than a bare String so that a type tag can never be
/// silently confused with a node id. Both are strings; they are not the same
/// kind of thing, and every bug where they got mixed would be invisible.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Type(String);

impl Type {
    /// Name a type.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name, for display and export.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Type {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A multiset of typed resources.
///
/// `BTreeMap` rather than `HashMap` so iteration order is deterministic. A
/// simulation whose trace reorders between runs is not reproducible, and
/// reproducibility is one of the two properties this crate exists to
/// guarantee.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bag(BTreeMap<Type, u64>);

impl Bag {
    /// An empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A bag holding `count` of one type.
    #[must_use]
    pub fn of(ty: impl Into<Type>, count: u64) -> Self {
        let mut bag = Self::new();
        bag.add(ty.into(), count);
        bag
    }

    /// Add resources of a type.
    pub fn add(&mut self, ty: Type, count: u64) {
        if count == 0 {
            return;
        }
        *self.0.entry(ty).or_insert(0) += count;
    }

    /// Remove up to `count`, returning how many were actually removed.
    ///
    /// Returns the amount rather than erroring, because "you asked for 3 and
    /// got 2" is information the caller needs to decide whether a `Demand::All`
    /// converter may fire at all.
    pub fn take(&mut self, ty: &Type, count: u64) -> u64 {
        let Some(held) = self.0.get_mut(ty) else {
            return 0;
        };
        let taken = count.min(*held);
        *held -= taken;
        if *held == 0 {
            self.0.remove(ty);
        }
        taken
    }

    /// How many of this type.
    #[must_use]
    pub fn count(&self, ty: &Type) -> u64 {
        self.0.get(ty).copied().unwrap_or(0)
    }

    /// Total across all types. Useful for conservation, meaningless for
    /// anything that cares which type it has.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.0.values().sum()
    }

    /// Nothing in it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Does this bag contain everything `other` asks for?
    ///
    /// The predicate behind `Demand::All`. A skill that needs three parallel
    /// review passes and has two has not run; it has half-run, and half-run is
    /// the state that produces confident wrong answers.
    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        other.0.iter().all(|(ty, n)| self.count(ty) >= *n)
    }

    /// Merge another bag in.
    pub fn absorb(&mut self, other: &Self) {
        for (ty, n) in &other.0 {
            self.add(ty.clone(), *n);
        }
    }

    /// Remove everything `other` asks for, if possible.
    ///
    /// All-or-nothing on purpose: a partial withdrawal would leave the ledger
    /// in a state no node intended, and the whole point of `Demand::All` is
    /// that partial is not a state.
    pub fn withdraw(&mut self, other: &Self) -> Option<Self> {
        if !self.contains(other) {
            return None;
        }
        let mut out = Self::new();
        for (ty, n) in &other.0 {
            let got = self.take(ty, *n);
            out.add(ty.clone(), got);
        }
        Some(out)
    }

    /// Drain everything, returning it.
    ///
    /// `must_use` is load-bearing rather than stylistic: discarding a drain
    /// destroys resources outside a Drain node, which is precisely the
    /// conservation leak INV-2 exists to catch. If you want an empty bag,
    /// assign one; do not throw away the contents of this one.
    #[must_use = "dropping drained resources is a conservation leak"]
    pub fn drain(&mut self) -> Self {
        Self(std::mem::take(&mut self.0))
    }

    /// Types present, in deterministic order.
    pub fn types(&self) -> impl Iterator<Item = &Type> {
        self.0.keys()
    }

    /// Per-type contents, in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (&Type, u64)> {
        self.0.iter().map(|(t, n)| (t, *n))
    }
}

impl FromIterator<(Type, u64)> for Bag {
    fn from_iter<I: IntoIterator<Item = (Type, u64)>>(iter: I) -> Self {
        let mut bag = Self::new();
        for (ty, n) in iter {
            bag.add(ty, n);
        }
        bag
    }
}

impl fmt::Display for Bag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("{}");
        }
        let parts: Vec<String> = self.0.iter().map(|(t, n)| format!("{n}×{t}")).collect();
        write!(f, "{{{}}}", parts.join(", "))
    }
}

//! P1.1 — the gate on typed resources.
//!
//! Properties, not examples. An example test tells you one input worked; a
//! property tells you a law holds. The laws here are the ones every layer
//! above silently assumes, and if any of them is false the conservation check
//! in INV-2 is measuring nothing.
//!
//! Generation is hand-rolled from a seeded LCG rather than a proptest
//! dependency, because INV-1 forbids `pact-core` from having dependencies and
//! a dev-dependency that shapes the tests is close enough to count. Same
//! reasoning as the simulator's PRNG: reproducible beats convenient.

use pact_core::{Bag, Type};

/// Deterministic case generation. A failing case must be reproducible from
/// its index alone, or debugging it becomes archaeology.
struct Gen(u64);

impl Gen {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 11
    }

    /// A bag over a small type alphabet, so collisions between types are
    /// common. A generator that never produces overlapping types would never
    /// exercise the multiset behaviour that matters.
    fn bag(&mut self) -> Bag {
        const ALPHABET: [&str; 4] = ["M", "M_plan", "State", "M_creative"];
        let mut bag = Bag::new();
        let kinds = (self.next() % 4) as usize;
        for _ in 0..kinds {
            let ty = ALPHABET[(self.next() % 4) as usize];
            let count = self.next() % 5;
            bag.add(Type::new(ty), count);
        }
        bag
    }
}

const CASES: u64 = 1_000;

#[test]
fn withdraw_is_all_or_nothing() {
    // The law `Demand::All` rests on. A converter that needs three tokens and
    // takes two has not run; it has half-run, and half-run is the state that
    // produces confident wrong answers downstream.
    let mut gen = Gen::new(0xA11);
    for case in 0..CASES {
        let source = gen.bag();
        let want = gen.bag();
        let mut trial = source.clone();
        match trial.withdraw(&want) {
            Some(got) => {
                assert_eq!(got, want, "case {case}: partial withdrawal returned");
                for (ty, n) in want.iter() {
                    assert_eq!(
                        trial.count(ty),
                        source.count(ty) - n,
                        "case {case}: wrong amount removed for {ty}"
                    );
                }
            }
            None => assert_eq!(
                trial, source,
                "case {case}: failed withdrawal must leave the source untouched"
            ),
        }
    }
}

#[test]
fn contains_is_a_partial_order() {
    let mut gen = Gen::new(0xC0DE);
    for case in 0..CASES {
        let a = gen.bag();
        let b = gen.bag();
        let c = gen.bag();

        assert!(a.contains(&a), "case {case}: not reflexive");

        if a.contains(&b) && b.contains(&a) {
            assert_eq!(a, b, "case {case}: mutual containment without equality");
        }
        if a.contains(&b) && b.contains(&c) {
            assert!(a.contains(&c), "case {case}: not transitive");
        }
    }
}

#[test]
fn absorb_then_withdraw_is_identity() {
    // The round trip every transfer in the engine performs. If this leaks, so
    // does conservation, and conservation is what makes every economy number
    // downstream mean anything.
    let mut gen = Gen::new(0xBEEF);
    for case in 0..CASES {
        let original = gen.bag();
        let moved = gen.bag();

        let mut trial = original.clone();
        trial.absorb(&moved);
        let back = trial
            .withdraw(&moved)
            .unwrap_or_else(|| panic!("case {case}: absorbed bag could not be withdrawn"));

        assert_eq!(back, moved, "case {case}: withdrew something else");
        assert_eq!(trial, original, "case {case}: round trip was lossy");
    }
}

#[test]
fn totals_are_conserved_across_transfer() {
    let mut gen = Gen::new(0xF00D);
    for case in 0..CASES {
        let mut from = gen.bag();
        let mut to = gen.bag();
        let before = from.total() + to.total();

        let want = gen.bag();
        if let Some(moved) = from.withdraw(&want) {
            to.absorb(&moved);
        }
        assert_eq!(
            from.total() + to.total(),
            before,
            "case {case}: a transfer changed the total"
        );
    }
}

#[test]
fn empty_bag_edges_do_not_panic() {
    let mut empty = Bag::new();
    let ty = Type::new("M");

    assert!(empty.is_empty());
    assert_eq!(empty.total(), 0);
    assert_eq!(empty.count(&ty), 0);
    assert_eq!(empty.take(&ty, 5), 0, "taking from empty must yield zero");
    // Every bag contains the empty bag. This is what lets a converter with no
    // inputs fire rather than starving forever.
    assert!(empty.contains(&Bag::new()));
    assert!(Bag::of("M", 3).contains(&Bag::new()));
    assert!(empty.withdraw(&Bag::new()).is_some());
    assert!(empty.withdraw(&Bag::of("M", 1)).is_none());
}

#[test]
fn adding_zero_is_not_a_type_present() {
    // A pool that once held zero `M_plan` must not report `M_plan` as a type
    // it carries. Otherwise the analysis layer counts phantom edges: a type
    // with a producer and no resource is indistinguishable from a real one.
    let mut bag = Bag::new();
    bag.add(Type::new("M_plan"), 0);
    assert!(bag.is_empty(), "adding zero created a phantom type");
    assert_eq!(bag.types().count(), 0);

    // And draining the last token removes the key rather than leaving a zero.
    let mut bag = Bag::of("State", 1);
    assert_eq!(bag.take(&Type::new("State"), 1), 1);
    assert!(bag.is_empty(), "a drained type lingered at zero");
    assert_eq!(bag.types().count(), 0);
}

#[test]
fn iteration_order_is_deterministic() {
    // INV-3. A trace that reorders between runs is not reproducible, and the
    // barcode compares traces across generations. `BTreeMap` is load-bearing
    // here, not a style preference.
    let build = || {
        let mut bag = Bag::new();
        for ty in ["State", "M", "M_creative", "M_plan"] {
            bag.add(Type::new(ty), 2);
        }
        bag.types().map(ToString::to_string).collect::<Vec<_>>()
    };
    assert_eq!(build(), build());
    assert_eq!(build(), vec!["M", "M_creative", "M_plan", "State"]);
}

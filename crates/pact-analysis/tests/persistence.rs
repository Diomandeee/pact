//! P4.4 — a fabricated face is rejected at admission, not at report time.
//!
//! That distinction is the whole gate. A fabrication flagged later has already
//! entered the barcode, and every bar computed after it describes a complex
//! that never existed. Rejecting at the door means the module only ever
//! contains generations that came from somewhere.

use pact_analysis::{Complex, Filtration};
use std::collections::BTreeMap;

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

#[test]
fn a_fabricated_face_is_refused_at_the_door() {
    // The gate. `c` appears in the next generation and nothing in the previous
    // one maps to it, so the compaction invented a connection.
    let first = Complex::new().with(&["a", "b"]);
    let mut filtration = Filtration::seed(first);

    let invented = Complex::new().with(&["a", "b"]).with(&["b", "c"]);
    let fabrication = filtration
        .admit(invented, &map(&[("a", "a"), ("b", "b")]))
        .expect_err("c came from nowhere");

    assert_eq!(fabrication.unmapped, vec!["c".to_string()]);
    assert_eq!(fabrication.generation, 1);
    assert!(
        fabrication.to_string().contains("came from nowhere"),
        "{fabrication}"
    );

    // And it was refused, not recorded. The filtration is unchanged, so no
    // barcode can be computed over a generation that was rejected.
    assert_eq!(filtration.len(), 1, "a refused generation must not be kept");
}

#[test]
fn a_legal_compaction_is_admitted() {
    // Without this the gate is satisfied by refusing everything, which is the
    // lesson every one-line gate here has taught.
    let first = Complex::new().with(&["a", "b"]).with(&["b", "c"]);
    let mut filtration = Filtration::seed(first);

    // Collapse b and c into one vertex. Legal: a simplicial map may collapse.
    let collapsed = Complex::new().with(&["a", "bc"]);
    filtration
        .admit(collapsed, &map(&[("a", "a"), ("b", "bc"), ("c", "bc")]))
        .expect("collapsing is what compaction does");

    assert_eq!(filtration.len(), 2);
}

#[test]
fn collapsing_is_legal_and_inventing_is_not() {
    // The two halves of D4 side by side, because the rule is easy to state as
    // "the map must be structure-preserving" and easy to implement as
    // "the map must be injective", which would forbid compaction entirely.
    let first = Complex::new()
        .with(&["a", "b"])
        .with(&["b", "c"])
        .with(&["c", "d"]);

    let mut collapsing = Filtration::seed(first.clone());
    assert!(
        collapsing
            .admit(
                Complex::new().with(&["x", "x"]),
                &map(&[("a", "x"), ("b", "x"), ("c", "x"), ("d", "x")])
            )
            .is_ok(),
        "collapsing four vertices to one is a legal compaction"
    );

    let mut inventing = Filtration::seed(first);
    assert!(
        inventing
            .admit(
                Complex::new().with(&["a", "e"]),
                &map(&[("a", "a"), ("b", "a"), ("c", "a"), ("d", "a")])
            )
            .is_err(),
        "one new vertex is a fabrication however small"
    );
}

#[test]
fn the_first_generation_needs_no_preimage() {
    // There is nothing behind it to fabricate against, and requiring a map
    // would make the module impossible to start.
    let mut filtration = Filtration::new();
    filtration
        .admit(Complex::new().with(&["a", "b", "c"]), &BTreeMap::new())
        .expect("the seed cannot fabricate");
    assert_eq!(filtration.len(), 1);
}

#[test]
fn a_component_bar_has_a_birth_and_a_death() {
    // b0 bars are identified, not counted: a component is a set of vertices,
    // so it can be followed.
    let mut f = Filtration::seed(Complex::new().with(&["a", "b"]).with(&["x", "y"]));
    assert_eq!(f.generations()[0].betti().b0, 2, "two components at birth");

    // Join them. One component continues, the other dies into it.
    f.admit(
        Complex::new()
            .with(&["a", "b"])
            .with(&["x", "y"])
            .with(&["b", "x"]),
        &map(&[("a", "a"), ("b", "b"), ("x", "x"), ("y", "y")]),
    )
    .expect("adding an edge invents no vertex");

    let bars = f.components_barcode();
    assert_eq!(bars.len(), 2, "two components were ever born: {bars:?}");

    let died: Vec<&pact_analysis::Bar> = bars.iter().filter(|b| b.death.is_some()).collect();
    assert_eq!(died.len(), 1, "exactly one merged away: {bars:?}");
    assert_eq!(died[0].death, Some(1));

    let survivor = bars.iter().find(|b| b.death.is_none()).expect("one lives");
    assert_eq!(survivor.birth, 0);
    assert_eq!(survivor.length(f.len() - 1), 2);
}

#[test]
fn a_barcode_where_nothing_dies_is_not_selecting() {
    // The reason a barcode is the output rather than the complexes. Three
    // generations that never lose anything have measured nothing.
    let a = Complex::new().with(&["a", "b"]);
    let mut f = Filtration::seed(a.clone());
    let identity = map(&[("a", "a"), ("b", "b")]);
    f.admit(a.clone(), &identity).expect("identity is legal");
    f.admit(a, &identity).expect("identity is legal");

    let bars = f.components_barcode();
    assert!(
        bars.iter().all(|b| b.death.is_none()),
        "nothing died, so nothing was selected: {bars:?}"
    );

    // And the stall detector says so, which is Theorem 1 arriving empirically:
    // the loop is not producing structure and needs something from outside.
    assert_eq!(
        f.stalled_for(),
        2,
        "two generations with identical homology"
    );
}

#[test]
fn a_changing_filtration_does_not_read_as_stalled() {
    // The other half of the stall check.
    let mut f = Filtration::seed(Complex::new().with(&["a", "b"]).with(&["x", "y"]));
    f.admit(
        Complex::new()
            .with(&["a", "b"])
            .with(&["x", "y"])
            .with(&["b", "x"]),
        &map(&[("a", "a"), ("b", "b"), ("x", "x"), ("y", "y")]),
    )
    .expect("legal");

    assert_eq!(f.stalled_for(), 0, "b0 went 2 -> 1, so it is producing");
}

#[test]
fn cycle_counts_are_counted_and_the_docs_say_so() {
    // b1 bars are not identified, and the API says which one it is doing. A
    // barcode that quietly counted would look exactly like one that tracked,
    // which is the kind of difference nobody finds until a report is wrong.
    let hollow = Complex::new()
        .with(&["a", "b"])
        .with(&["b", "c"])
        .with(&["a", "c"]);
    let filled = Complex::new().with(&["a", "b", "c"]);

    let mut f = Filtration::seed(hollow);
    f.admit(filled, &map(&[("a", "a"), ("b", "b"), ("c", "c")]))
        .expect("filling invents no vertex");

    assert_eq!(
        f.cycle_counts(),
        vec![1, 0],
        "a hole opened and then closed, and that is all this claims"
    );
}

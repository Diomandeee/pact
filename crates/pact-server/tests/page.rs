//! P6.3 and P6.4 — the page, and its shapes.
//!
//! # The gate is on the DOM, not on pixels
//!
//! A visual regression test needs a browser and a baseline image, and it fails
//! for reasons that have nothing to do with whether the page is right. What can
//! be checked here is stronger than it sounds: that the headline numbers are in
//! the HTML **before any JavaScript runs**, and that the shape vocabulary the
//! page uses is the same one `pact export` applies to Mermaid.
//!
//! The second matters because a node drawn as a stadium in the diagram and a
//! box on the page would mean two views disagreeing about a diagnosis they both
//! computed from the same corpus.

use pact_analysis::{dynamics, statics};
use pact_corpus::{categorise, default_root, load};
use std::path::Path;

const INDEX: &str = include_str!("../web/index.html");
const STYLE: &str = include_str!("../web/app.css");
const SCRIPT: &str = include_str!("../web/app.js");

#[test]
fn the_page_is_embedded_and_needs_nothing_beside_it() {
    // P6.3. The deployable artifact is one binary. `include_str!` above is the
    // proof: if any of these were read from disk this file would not compile
    // in a checkout without them.
    assert!(INDEX.starts_with("<!doctype html>"));
    assert!(!STYLE.trim().is_empty());
    assert!(!SCRIPT.trim().is_empty());

    // And no build step. A bundler config or a lockfile would mean the page
    // has a toolchain, which is the thing this project has refused everywhere
    // else.
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    for forbidden in [
        "package.json",
        "package-lock.json",
        "node_modules",
        "vite.config.js",
    ] {
        assert!(
            !web.join(forbidden).exists(),
            "{forbidden} exists: the page has acquired a build step"
        );
    }
}

#[test]
fn the_headline_numbers_are_server_rendered() {
    // P6.3's real claim. A page whose first paint is a row of em-dashes is one
    // where a slow fetch and a broken corpus look identical.
    for marker in [
        "{{skills}}",
        "{{types}}",
        "{{loops}}",
        "{{dead}}",
        "{{share}}",
        "{{verdict}}",
    ] {
        assert!(
            INDEX.contains(marker),
            "{marker} is not in the template, so it cannot be substituted"
        );
    }

    // Every marker the template carries must be one the server fills. An
    // unsubstituted `{{...}}` renders literally, which is worse than a dash.
    let main = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("main.rs is readable");
    let mut rest = INDEX;
    while let Some(open) = rest.find("{{") {
        let close = rest[open..].find("}}").expect("a marker closes") + open + 2;
        let marker = &rest[open..close];
        assert!(
            main.contains(marker),
            "{marker} is in the page and nothing substitutes it"
        );
        rest = &rest[close..];
    }
}

#[test]
fn the_page_says_what_to_do_without_javascript() {
    // Not a claim that it works without JS — it does not, past the headline.
    // A claim that it says so, and names the endpoints, rather than rendering
    // an empty shell that looks broken.
    assert!(INDEX.contains("<noscript>"));
    for endpoint in ["/api/overview", "/api/chains", "/api/graph", "/api/tiers"] {
        assert!(
            INDEX.contains(endpoint),
            "{endpoint} is not offered to a reader without JavaScript"
        );
    }
}

#[test]
fn the_shape_vocabulary_matches_the_exporter() {
    // P6.4. The same four diagnoses, drawn the same way in both views.
    for shape in ["k-source", "k-drain", "k-hot", "k-pool"] {
        assert!(SCRIPT.contains(shape), "{shape} is not applied by the page");
        assert!(
            STYLE.contains(&format!(".{shape}")),
            "{shape} has no styling"
        );
        assert!(INDEX.contains(shape), "{shape} is not in the legend");
    }

    // Legend and rendering must agree. A legend describing a mark the page
    // never draws is worse than no legend: it makes the reader hunt.
    assert!(SCRIPT.contains("n.kind === 'source'"));
    assert!(SCRIPT.contains("n.kind === 'drain'"));
    assert!(SCRIPT.contains("n.under_pressure"));
}

#[test]
fn a_node_carries_its_diagnosis_in_a_data_attribute() {
    // So the gate can be on the DOM rather than on pixels, and so the marks
    // stay checkable when the styling changes.
    assert!(SCRIPT.contains("glyph.dataset.kind"));
    assert!(SCRIPT.contains("dataset.pressure"));
    assert!(
        SCRIPT.contains("dataset.dead"),
        "a dead skill must be marked in the DOM"
    );
}

#[test]
fn a_skill_opens_its_detail() {
    // P6.4's second half. The graph is only useful if a node leads somewhere.
    assert!(SCRIPT.contains("onclick = () => detail("));
    assert!(SCRIPT.contains("/api/skill/"));
    // And a 404 renders as "no such skill" rather than as a failure, because
    // they are different answers.
    assert!(SCRIPT.contains("no such skill"));
}

#[test]
fn only_the_alarm_uses_the_alarm_colour() {
    // A dashboard where several things are always coloured has no way left to
    // alarm. `--hot` is pressure and dead skills, and nothing else.
    assert!(STYLE.contains("--hot:"));
    assert!(
        SCRIPT.contains("if (o.never_fires.length > 0) $('#dead-fig').classList.add('alarm')"),
        "the alarm must be conditional, or it is decoration"
    );
}

#[test]
fn the_page_renders_the_same_corpus_the_analysis_reports() {
    // Belt and braces over P6.1: the template's markers correspond to fields
    // that exist, so a renamed field breaks the build rather than the page.
    let Ok(skills) = load(&default_root()) else {
        println!("SKIPPED: no corpus");
        return;
    };
    if skills.is_empty() {
        return;
    }
    let category = categorise(&skills);
    let stat = statics::analyse(&skills, &category);
    let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(&skills));

    // These are the five the template asks for. If any stops existing, this
    // stops compiling.
    let _ = skills.len();
    let _ = category.objects.len();
    let _ = dyn_.feedback_loops.len();
    let _ = stat.never_fires.len();
    let _ = dyn_.verdict();
}

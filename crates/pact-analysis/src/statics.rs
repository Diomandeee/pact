//! What the corpus is, without running it.
//!
//! Composability, dead ends, source-only types. All decidable from the
//! signatures, which is what makes them cheap enough to run on every edit.
//!
//! `prompt_algebra.py` is the oracle. P4.1 diffs against it.

use pact_corpus::{Category, Skill};
use std::collections::BTreeMap;

/// Two skills that could be chained, and whether they can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition {
    /// Upstream.
    pub from: String,
    /// Downstream.
    pub to: String,
}

/// The static picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statics {
    /// Types nothing consumes: work arrives and cannot continue.
    pub dead_ends: Vec<String>,
    /// Types nothing produces: reachable only as a starting point.
    pub source_only: Vec<String>,
    /// Skills whose input type nothing produces, so they can never fire.
    ///
    /// `Subject` is excluded, and that exclusion is the whole reason this is
    /// not the same list as "consumes a source-only type": `Subject` is what a
    /// person brings, so a skill reading one is waiting, not broken.
    pub never_fires: Vec<String>,
    /// Skills sharing a signature, **two or more only**.
    ///
    /// A shortlist of candidates for the same morphism, not a census. A class
    /// of one is every skill that exists and says nothing; the oracle filters
    /// them out for that reason, and matching it is not a compromise.
    pub signature_classes: BTreeMap<String, Vec<String>>,
    /// How many ordered pairs compose.
    pub composable_pairs: usize,
}

/// Read the static structure of a corpus.
#[must_use]
pub fn analyse(skills: &[Skill], category: &Category) -> Statics {
    let mut signature_classes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for skill in skills {
        let key = format!(
            "{} -> {}",
            skill.input,
            skill
                .outputs
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" | ")
        );
        signature_classes
            .entry(key)
            .or_default()
            .push(skill.name.clone());
    }
    for names in signature_classes.values_mut() {
        names.sort();
    }
    // A class of one is not a candidate for anything.
    signature_classes.retain(|_, names| names.len() > 1);

    // A reader of `M` fires when any `M_*` is produced: the economy's degrees
    // carry the coercions (P3.6), the category's do not.
    let economy = pact_corpus::economy_degrees(category);
    let never_fires = skills
        .iter()
        .filter(|s| {
            s.input.as_str() != "Subject"
                && economy.get(&s.input).is_none_or(|d| d.produced_by == 0)
        })
        .map(|s| s.name.clone())
        .collect();

    let composable_pairs = skills
        .iter()
        .flat_map(|a| {
            skills
                .iter()
                .filter(move |b| a.name != b.name && a.outputs.contains(&b.input))
        })
        .count();

    Statics {
        dead_ends: category.dead_ends.iter().map(ToString::to_string).collect(),
        source_only: category
            .source_only
            .iter()
            .map(ToString::to_string)
            .collect(),
        never_fires,
        signature_classes,
        composable_pairs,
    }
}

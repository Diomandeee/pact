//! Diagram export.
//!
//! # Node shape carries the diagnosis
//!
//! A diagram that draws every node the same is a picture of a graph. The
//! shapes here mean something: a source is a stadium, a drain is a trapezoid,
//! a pool under pressure is marked, and a skill that can never fire is marked
//! differently. Someone reading it should see the finding without reading the
//! report.
//!
//! Mermaid, because it renders in a Markdown file, a GitHub comment and a
//! terminal preview without a toolchain. The output is text and nothing here
//! writes a file: what a person does with a diagram is their business, and a
//! tool that decides where it lands is one that has to be argued with.

use pact_analysis::{dynamics, statics};
use pact_corpus::{categorise, Skill};
use std::fmt::Write;

/// Render a corpus as a Mermaid flowchart.
#[must_use]
pub fn mermaid(skills: &[Skill]) -> String {
    let category = categorise(skills);
    let stat = statics::analyse(skills, &category);
    let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(skills));

    let mut out = String::from("flowchart LR\n");

    for node in &dyn_.nodes {
        let id = sanitise(&node.id);
        let pressure = node
            .pressure
            .map_or_else(String::new, |p| format!("<br/>{p}"));
        let label = format!("{}{pressure}", node.id);
        // Shape is the diagnosis. A stadium is where work enters, a trapezoid
        // is where it leaves, and a pool is a box that may be flagged.
        let shape = match node.kind {
            dynamics::Machination::Source => format!("{id}([\"{label}\"])"),
            dynamics::Machination::Drain => format!("{id}[/\"{label}\"\\]"),
            dynamics::Machination::Pool => {
                if dyn_.unbounded.iter().any(|u| u.id == node.id) {
                    format!("{id}[[\"{label}\"]]")
                } else {
                    format!("{id}[\"{label}\"]")
                }
            }
        };
        let _ = writeln!(out, "  {shape}");
    }

    // Skills, as edges between the type they read and each type they emit.
    // Drawing them as edges rather than nodes halves the diagram and loses
    // nothing: a skill IS the arrow from one type to another.
    for skill in skills {
        let dead = stat.never_fires.contains(&skill.name);
        for out_ty in &skill.outputs {
            let arrow = if dead { "-.->" } else { "-->" };
            let label = if dead {
                format!("{} (never fires)", skill.name)
            } else {
                skill.name.clone()
            };
            let _ = writeln!(
                out,
                "  {} {arrow}|\"{label}\"| {}",
                sanitise(skill.input.as_str()),
                sanitise(out_ty.as_str())
            );
        }
    }

    // Refinement (P3.6): a produced `M_*` subtype is read as `M`. Not a skill, so a thick arrow.
    for sub in pact_corpus::coercions(&category) {
        let _ = writeln!(out, "  {} ==>|\"coerce:{sub}\"| M", sanitise(sub.as_str()));
    }

    out.push_str("\n  %% stadium: work enters here. trapezoid: work leaves.\n");
    out.push_str("  %% double box: a pool under pressure. dotted: a skill that can never fire.\n");
    out.push_str("  %% thick: a coercion, a subtype read as M.\n");
    out
}

/// Mermaid ids cannot carry `^`, `:` or spaces, and a type name can.
fn sanitise(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

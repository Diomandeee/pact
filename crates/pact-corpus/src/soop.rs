//! Read SOOP-2 frontmatter as a category, and as a graph.
//!
//! A `SKILL.md` carries a typed block: `input_type`, `output_type`,
//! `category`, `effects`, `idempotent`, `commutes_with`. That is not
//! documentation, it is a presentation of a category — objects are types,
//! morphisms are skills, composition is chaining — which is why importing a
//! corpus is a translation rather than an interpretation.
//!
//! # Bug-for-bug with the oracle
//!
//! `~/.claude/skills/divergent-rail/scripts/prompt_algebra.py` is the
//! reference and P3.1 is a differential test against it. Two details are
//! copied deliberately rather than improved:
//!
//! 1. **Frontmatter is everything between the first two `---`.** Not a YAML
//!    parse. A field is a line beginning `name:`; the value is the rest of the
//!    line, trimmed.
//! 2. **A file counts only if it contains `type_spec_version`** anywhere,
//!    including in its body.
//!
//! Improving either would make the Rust disagree with the Python, and then the
//! differential test reports a difference that is really a fix. When they
//! disagree, find out which is wrong first; twice so far the older
//! implementation has been correct.

use pact_core::{Bag, Graph, Node, ResourceEdge, Signature, Type};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The default place a corpus lives.
///
/// # Panics
///
/// If `HOME` is unset. A corpus root that cannot be located is not a
/// recoverable condition for a tool whose entire job is reading one.
#[must_use]
pub fn default_root() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME must be set to find the corpus"))
        .join(".claude/skills")
}

/// One skill, as its frontmatter declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    /// The directory name, which is the skill's name.
    pub name: String,
    /// What it reads. `?` when undeclared, which is itself a finding.
    pub input: Type,
    /// What it may emit. `output_type: A | B` gives two.
    pub outputs: Vec<Type>,
    /// SOOP-2 category: Generator, Reducer, Distributor, Effector, Auditor.
    pub category: Option<String>,
    /// Declared effects, verbatim.
    pub effects: Option<String>,
    /// Whether `f ∘ f = f`.
    pub idempotent: bool,
    /// Skills this one commutes with.
    pub commutes_with: Vec<String>,
    /// For a chain: its members, declared in order (`members: [a, b, c]`).
    /// Empty when undeclared, in which case the factoring falls back to the
    /// prose scan. Declared beats scanned: a chain's word should depend on
    /// what it runs, not on which names its documentation happens to mention.
    pub members: Vec<String>,
    /// `iterates: true` — the skill repeats until a condition. The `*`
    /// combinator, declared in the frontmatter instead of inferred from a
    /// list. The list ([`crate::chains::ITERATORS`]) remains the fallback for
    /// a corpus that has not declared it.
    pub iterates: bool,
    /// Where it was read from.
    pub path: PathBuf,
}

impl Skill {
    /// Parse one `SKILL.md`, or `None` if it carries no type block.
    #[must_use]
    pub fn parse(path: &Path, text: &str) -> Option<Self> {
        if !text.contains("type_spec_version") {
            return None;
        }
        // Everything between the first two `---`, matching the oracle's
        // `text.split("---")[1]`.
        let head = if text.starts_with("---") {
            text.split("---").nth(1).unwrap_or("")
        } else {
            let cut = text.char_indices().nth(4000).map_or(text.len(), |(i, _)| i);
            &text[..cut]
        };

        let name = path
            .parent()
            .and_then(|p| p.file_name())
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

        let input = field(head, "input_type").unwrap_or_else(|| "?".to_string());
        let output = field(head, "output_type").unwrap_or_else(|| "?".to_string());

        Some(Self {
            name,
            input: Type::new(input),
            // `Report | silent` means the skill may return nothing. Both are
            // objects, and treating the union as one type is how a legal chain
            // gets reported as broken.
            outputs: output.split('|').map(|t| Type::new(t.trim())).collect(),
            category: field(head, "category"),
            effects: field(head, "effects"),
            idempotent: field(head, "idempotent").as_deref() == Some("true"),
            commutes_with: list_field(head, "commutes_with"),
            members: list_field(head, "members"),
            iterates: field(head, "iterates").as_deref() == Some("true"),
            path: path.to_path_buf(),
        })
    }

    /// The signature the engine sees.
    ///
    /// One output, because a `Signature` is what a firing actually emits and
    /// alternatives are a fact about the interface. The others survive on
    /// [`Self::outputs`], which is what composition consults.
    #[must_use]
    pub fn signature(&self) -> Signature {
        Signature {
            consumes: Bag::of(self.input.clone(), 1),
            produces: self
                .outputs
                .first()
                .map_or_else(Bag::new, |t| Bag::of(t.clone(), 1)),
        }
    }
}

fn field(head: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}:");
    head.lines()
        .find(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].trim().to_string())
        .filter(|v| !v.is_empty())
}

fn list_field(head: &str, name: &str) -> Vec<String> {
    field(head, name).map_or_else(Vec::new, |raw| {
        raw.trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect()
    })
}

/// Every skill under `root`, in name order.
///
/// # Errors
///
/// Only if the directory cannot be listed. An unreadable individual file is
/// skipped rather than fatal: one bad file should not make a corpus
/// unanalysable, and the count difference is visible in the report.
pub fn load(root: &Path) -> std::io::Result<Vec<Skill>> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Sorted, because the oracle globs and sorts, and because a corpus that
    // reorders between runs cannot be diffed across generations.
    dirs.sort();

    let mut found = Vec::new();
    for dir in dirs {
        let file = dir.join("SKILL.md");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        if let Some(skill) = Skill::parse(&file, &text) {
            found.push(skill);
        }
    }
    Ok(found)
}

// ─────────────────────────────────────────────────────────── the category

/// How many skills produce and consume one type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Degree {
    /// How many skills emit it.
    pub produced_by: usize,
    /// How many skills read it.
    pub consumed_by: usize,
}

/// The corpus, read as a category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// Every type appearing on either side of a signature.
    pub objects: Vec<Type>,
    /// Degree per object.
    pub degrees: BTreeMap<Type, Degree>,
    /// Types nothing consumes. Work arrives and cannot continue.
    pub dead_ends: Vec<Type>,
    /// Types nothing produces. Reachable only as a starting point.
    pub source_only: Vec<Type>,
    /// How many skills were read.
    pub morphisms: usize,
}

/// Read a loaded corpus as a category.
#[must_use]
pub fn categorise(skills: &[Skill]) -> Category {
    let mut produced: BTreeMap<Type, BTreeSet<String>> = BTreeMap::new();
    let mut consumed: BTreeMap<Type, BTreeSet<String>> = BTreeMap::new();

    for skill in skills {
        consumed
            .entry(skill.input.clone())
            .or_default()
            .insert(skill.name.clone());
        for out in &skill.outputs {
            produced
                .entry(out.clone())
                .or_default()
                .insert(skill.name.clone());
        }
    }

    let objects: Vec<Type> = produced
        .keys()
        .chain(consumed.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let degrees: BTreeMap<Type, Degree> = objects
        .iter()
        .map(|ty| {
            (
                ty.clone(),
                Degree {
                    produced_by: produced.get(ty).map_or(0, BTreeSet::len),
                    consumed_by: consumed.get(ty).map_or(0, BTreeSet::len),
                },
            )
        })
        .collect();

    let dead_ends = objects
        .iter()
        .filter(|ty| degrees[*ty].consumed_by == 0)
        .cloned()
        .collect();
    let source_only = objects
        .iter()
        .filter(|ty| degrees[*ty].produced_by == 0)
        .cloned()
        .collect();

    Category {
        objects,
        degrees,
        dead_ends,
        source_only,
        morphisms: skills.len(),
    }
}

/// The material subtypes some skill produces, each a free coercion `M_x ⇒ M` in the economy (P3.6).
///
/// Composition honours refinement ([`crate::chains::compatible`], `SKILL_TYPES` §4.3 rule 2): an
/// `M_plan` output satisfies an `M` input. The economy has to say the same thing, or a reader of `M`
/// counts as starved while dozens of skills emit `M_*`. One converter per produced subtype, reading it
/// and emitting `M`, makes the two views agree. A subtype nothing produces gets none (a coercion from
/// a pool nothing fills carries nothing), and without an `M` object there is nothing to coerce into.
/// The category itself stays nominal: dead ends, source-only types and degrees are what the skills
/// declare, and P3.1 diffs them against `prompt_algebra.py` unchanged.
#[must_use]
pub fn coercions(category: &Category) -> Vec<Type> {
    if !category.degrees.contains_key(&Type::from("M")) {
        return Vec::new();
    }
    category
        .objects
        .iter()
        .filter(|ty| ty.as_str().starts_with("M_") && category.degrees[*ty].produced_by > 0)
        .cloned()
        .collect()
}

/// Degrees as the economy counts them: the category's, plus each coercion as one more consumer of its
/// subtype and one more producer of `M`. Pressure, never-fire and the graph all read these.
#[must_use]
pub fn economy_degrees(category: &Category) -> BTreeMap<Type, Degree> {
    let mut degrees = category.degrees.clone();
    let subtypes = coercions(category);
    for ty in &subtypes {
        if let Some(d) = degrees.get_mut(ty) {
            d.consumed_by += 1;
        }
    }
    if let Some(general) = degrees.get_mut(&Type::from("M")) {
        general.produced_by += subtypes.len();
    }
    degrees
}

// ─────────────────────────────────────────────────────────── the graph

/// Build the engine graph: types are pools, skills are converters.
///
/// Bipartite on purpose. A type with many producers and one consumer is a pool
/// under pressure, and that is the same statement in both readings — which is
/// what makes the economy analysis and the category analysis two views of one
/// object rather than two models that have to be kept in step.
///
/// Type nodes are prefixed `type:` and skill nodes `skill:`, because a type
/// tag and a node id are both strings and are not the same kind of thing.
#[must_use]
pub fn to_graph(skills: &[Skill]) -> Graph {
    let mut graph = Graph::new();
    let category = categorise(skills);

    for ty in &category.objects {
        graph.add(
            Node::pool(format!("type:{ty}"))
                .with_meta("kind", "type")
                .with_meta("type", ty.as_str()),
        );
    }

    for skill in skills {
        let id = format!("skill:{}", skill.name);
        let mut node = Node::converter(id.clone(), skill.signature())
            .with_meta("kind", "skill")
            .with_meta("skill", &skill.name);
        if let Some(category) = &skill.category {
            node = node.with_meta("category", category);
        }
        if skill.idempotent {
            node = node.with_meta("idempotent", "true");
        }
        graph.add(node);

        graph.connect(ResourceEdge::all(
            format!("type:{}", skill.input),
            id.clone(),
        ));
        for out in &skill.outputs {
            graph.connect(ResourceEdge::all(id.clone(), format!("type:{out}")));
        }
    }

    // Refinement as converters (P3.6): `coerce:M_plan` reads `M_plan` and emits `M`, so a reader of `M`
    // is fed by every `M_*` producer in the simulator exactly as it is in composition.
    for sub in coercions(&category) {
        let id = format!("coerce:{sub}");
        graph.add(
            Node::converter(id.clone(), Signature::simple(sub.clone(), "M"))
                .with_meta("kind", "coercion")
                .with_meta("refines", sub.as_str()),
        );
        graph.connect(ResourceEdge::all(format!("type:{sub}"), id.clone()));
        graph.connect(ResourceEdge::all(id, "type:M"));
    }
    graph
}

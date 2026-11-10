//! Factor a chain into a word.
//!
//! A chain names skills; a word names how they compose. `decompose.py` is the
//! oracle and P3.2 is a differential test against it.
//!
//! ```text
//!  ⊗  broadcast   in(A) == in(B)       both read the same upstream
//!  ▸  sequence    in(B) ∈ out(A)       work flows onward
//!  ✗  broken      neither              it does not compose
//!  ⊕  select      a member reads M^n
//!  *  iterate     a member is an iterating effector
//!  ↺  repeat      in(first) ∈ out(last)
//! ```
//!
//! Types decide, not prose. Two consecutive members reading the same input are
//! parallel however the documentation describes them, and that one rule is what
//! caught a chain in the live corpus calling a fan-out a sequence.
//!
//! # Members come from prose, and that weakness is copied on purpose
//!
//! The oracle finds a chain's members by scanning its `SKILL.md` for the names
//! of known skills, ordered by **first mention**. So a chain's word depends on
//! how its documentation is written and not only on what it does: naming an
//! unrelated skill in a closing paragraph adds it to the member list, and
//! naming one late can move it to the end and change whether the loop closes.
//!
//! Found the hard way. The first draft of `chain:generational` scored 3/4
//! instead of 4/4 because its final section mentioned two skills it does not
//! use, and the bare word "chain", which is itself a skill name.
//!
//! Reproduced exactly. A differential test against a fixed oracle has to match
//! the oracle including the parts that are wrong, or every disagreement needs
//! investigating twice: once to find what differs, and once to find whether
//! the difference was deliberate.
//!
//! **Since 2026-11-03 a chain may declare its members** (`members: [a, b]` in
//! the SOOP-2 block), and a declared list wins over the prose scan. The scan
//! survives as the fallback for undeclared chains, so the oracle and this
//! module still agree on a corpus that has declared nothing. A declared member
//! that is not a typed skill is a defect: noted, dropped from the word, and
//! the chain does not typecheck.
//!
//! A declaration also carries **shape**: `(a | b)` is a fan-out, everything
//! else is sequence. That resolves the one ambiguity types alone cannot — two
//! consecutive readers of `M` are parallel under the scan's rule, whether or
//! not the second was meant to read the first's output — by letting the chain
//! say which it meant and letting the types check it. A declared sequence
//! whose pair does not compose is `✗`; a declared fan-out whose branches read
//! different types is `✗`; a stage after a fan-out must be fed by every
//! branch. The declaration decides intent; the types still decide legality.
//!
//! **Composition uses refinement.** `SKILL_TYPES_v1.md` §4.3 rule 2 says a
//! specific material type satisfies the general one: `M_plan ⇒ M`. The
//! factoring honours it ([`compatible`]), and since P3.6 so does the economy:
//! one free coercion converter per produced subtype ([`crate::coercions`]),
//! in `to_graph`, the dynamics and the oracle `prompt_economy.py` alike.

use crate::soop::Skill;
use pact_core::Type;
use std::collections::BTreeMap;

/// Skills whose semantics are "repeat until a condition". The `*` combinator.
///
/// A hard-coded set in the oracle, and hard-coded here for the same reason it
/// is there: nothing in SOOP-2 declares "this one iterates", so the fact lives
/// in a list until the frontmatter can carry it. That is a gap in the type
/// system rather than in the analysis.
pub const ITERATORS: [&str; 5] = [
    "pulse",
    "meta:hydra",
    "ops:autopilot",
    "evo:evolve",
    "meta:omega",
];

/// Does an output satisfy an input? Equality, or refinement: a specific
/// material type (`M_` prefix) satisfies the general `M`. Nothing else is
/// widened — `M^n` is a compound, `Subject` and `State` are not material, and
/// the general never satisfies the specific (`M ⇒ M_plan` is false).
#[must_use]
pub fn compatible(out: &Type, input: &Type) -> bool {
    out == input || (input.as_str() == "M" && out.as_str().starts_with("M_"))
}

/// Does this skill iterate? Declared in its frontmatter, or on the fallback
/// list for a corpus that has not declared it.
#[must_use]
pub fn iterates(skill: &Skill) -> bool {
    skill.iterates || ITERATORS.contains(&skill.name.as_str())
}

/// How a pair of consecutive members relates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// `⊗` — both read the same upstream, so they are parallel.
    Broadcast,
    /// `▸` — the second reads what the first may emit.
    Sequence,
    /// `✗` — neither. The chain is mis-specified here.
    Broken,
}

impl Step {
    /// The symbol the analysis writes.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Broadcast => "⊗",
            Self::Sequence => "▸",
            Self::Broken => "✗",
        }
    }
}

/// Which canonical stages a chain has.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stages {
    /// `⊗` — the plan fans out.
    pub expand: bool,
    /// `*` — an effector iterates.
    pub execute: bool,
    /// `⊕` — results collapse to one.
    pub select: bool,
    /// `↺` — the chain returns to its own start.
    pub repeat: bool,
}

impl Stages {
    /// How many of the four are present.
    #[must_use]
    pub fn present(self) -> usize {
        usize::from(self.expand)
            + usize::from(self.execute)
            + usize::from(self.select)
            + usize::from(self.repeat)
    }
}

/// One chain, factored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factoring {
    /// The chain's name.
    pub chain: String,
    /// Its members, in first-mention order.
    pub members: Vec<String>,
    /// The word, e.g. `⊗ ▸ * ▸ ⊕`.
    pub word: String,
    /// Which canonical stages it has.
    pub stages: Stages,
    /// Whether every consecutive pair composes.
    pub typechecks: bool,
    /// Why a pair did not compose, or why a pair is parallel.
    pub notes: Vec<String>,
    /// The declared shape, as stages of member names; `None` for a chain
    /// factored from the prose scan, whose shape the types inferred.
    pub shape: Option<Vec<Vec<String>>>,
}

impl Factoring {
    /// `n/4`, as the analysis writes it.
    #[must_use]
    pub fn completeness(&self) -> String {
        format!("{}/4", self.stages.present())
    }
}

/// First position where `needle` appears not adjoined to `[\w:-]`.
fn find_whole(haystack: &str, needle: &str) -> Option<usize> {
    let boundary = |c: char| !(c.is_alphanumeric() || c == '_' || c == ':' || c == '-');
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(needle) {
        let at = from + rel;
        let before_ok = at == 0 || haystack[..at].chars().next_back().is_some_and(boundary);
        let after = at + needle.len();
        let after_ok =
            after >= haystack.len() || haystack[after..].chars().next().is_some_and(boundary);
        if before_ok && after_ok {
            return Some(at);
        }
        from = at + needle.len().max(1);
    }
    None
}

/// Which known skills does this text name, in order of first mention?
///
/// Word-boundary matched so `pulse` does not match `pulse:start`, exactly as
/// the oracle does it. See the module docs for why this is prose-driven and
/// why that is copied rather than fixed.
#[must_use]
pub fn members(text: &str, known: &[String]) -> Vec<String> {
    let mut hits: Vec<(usize, &String)> = known
        .iter()
        .filter(|name| !name.starts_with("chain:"))
        .filter_map(|name| find_whole(text, name).map(|at| (at, name)))
        .collect();
    hits.sort_by_key(|(at, _)| *at);
    hits.into_iter().map(|(_, n)| n.clone()).collect()
}

/// Factor a member sequence into a word.
#[must_use]
pub fn factor(chain: &str, sequence: &[String], by_name: &BTreeMap<String, Skill>) -> Factoring {
    let mut symbols: Vec<&'static str> = Vec::new();
    let mut notes = Vec::new();
    let mut broken = false;

    for pair in sequence.windows(2) {
        let (Some(a), Some(b)) = (by_name.get(&pair[0]), by_name.get(&pair[1])) else {
            continue;
        };
        // Order matters. Two members reading the same type are parallel even
        // when the second could also follow the first, so equality of inputs is
        // checked BEFORE composition. Swapping those reclassifies every fan-out
        // whose branches happen to chain.
        let step = if a.input == b.input {
            notes.push(format!(
                "{} and {} both consume {}: parallel, not sequential",
                pair[0], pair[1], a.input
            ));
            Step::Broadcast
        } else if a.outputs.iter().any(|out| compatible(out, &b.input)) {
            Step::Sequence
        } else {
            broken = true;
            notes.push(format!(
                "{} yields {}, {} wants {}: does not compose",
                pair[0],
                a.outputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("|"),
                pair[1],
                b.input
            ));
            Step::Broken
        };
        symbols.push(step.symbol());
    }

    let has_merge = sequence
        .iter()
        .filter_map(|s| by_name.get(s))
        .any(|s| s.input.as_str() == "M^n");
    if has_merge {
        symbols.push("⊕");
    }
    let has_iter = sequence.iter().any(|s| {
        by_name
            .get(s)
            .map_or_else(|| ITERATORS.contains(&s.as_str()), iterates)
    });
    if has_iter {
        symbols.push("*");
    }
    let repeat = match (sequence.first(), sequence.last()) {
        (Some(first), Some(last)) => by_name
            .get(first)
            .zip(by_name.get(last))
            .is_some_and(|(f, l)| l.outputs.iter().any(|out| compatible(out, &f.input))),
        _ => false,
    };
    if repeat {
        symbols.push("↺");
    }

    Factoring {
        chain: chain.to_string(),
        members: sequence.to_vec(),
        word: if symbols.is_empty() {
            "(single)".to_string()
        } else {
            symbols.join(" ")
        },
        stages: Stages {
            expand: symbols.contains(&"⊗"),
            execute: has_iter,
            select: has_merge,
            repeat,
        },
        typechecks: !broken,
        notes,
        shape: None,
    }
}

/// One stage of a declared chain: a single member, or a fan-out of several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// `name` — one member.
    One(String),
    /// `(a | b | c)` — branches that all read the same upstream.
    Many(Vec<String>),
}

impl Stage {
    fn names(&self) -> Vec<String> {
        match self {
            Self::One(n) => vec![n.clone()],
            Self::Many(v) => v.clone(),
        }
    }
}

/// Read a `members:` declaration into stages. A token wrapped in parentheses
/// and split on `|` is a fan-out; any other token is one member. The
/// frontmatter list is already comma-split, so a group is one token.
#[must_use]
pub fn stages(declared: &[String]) -> Vec<Stage> {
    declared
        .iter()
        .map(|token| {
            let t = token.trim();
            if t.starts_with('(') && t.ends_with(')') {
                Stage::Many(
                    t[1..t.len() - 1]
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect(),
                )
            } else {
                Stage::One(t.to_string())
            }
        })
        .collect()
}

/// Resolve a declaration to the skills it names, keeping its shape. Unknown
/// names are noted and break the chain.
fn declared_shape<'a>(
    chain: &str,
    declared: &[String],
    by_name: &'a BTreeMap<String, Skill>,
    notes: &mut Vec<String>,
    broken: &mut bool,
) -> (Vec<Vec<&'a Skill>>, Vec<String>) {
    let mut shape: Vec<Vec<&Skill>> = Vec::new();
    let mut members = Vec::new();
    for stage in stages(declared) {
        let mut group = Vec::new();
        for name in stage.names() {
            if let Some(skill) = by_name.get(&name) {
                group.push(skill);
                members.push(name);
            } else {
                notes.push(format!(
                    "{chain} declares {name}, which is not a typed skill"
                ));
                *broken = true;
            }
        }
        if !group.is_empty() {
            shape.push(group);
        }
    }
    (shape, members)
}

/// Factor a chain from its **declared** members and shape.
///
/// Names that are not typed skills are a defect in the declaration: each is
/// noted, left out of the word, and the chain does not typecheck. Silently
/// dropping them would let a chain declare a member that does not exist and
/// still read as legal. A declared sequence must compose; a declared fan-out
/// must read one type; the stage after a fan-out must be fed by every branch.
#[must_use]
pub fn factor_declared(
    chain: &str,
    declared: &[String],
    by_name: &BTreeMap<String, Skill>,
) -> Factoring {
    let mut notes = Vec::new();
    let mut broken = false;
    let mut symbols: Vec<&'static str> = Vec::new();

    let (shape, members) = declared_shape(chain, declared, by_name, &mut notes, &mut broken);

    for (i, group) in shape.iter().enumerate() {
        // Within a fan-out every branch reads the same type, or it was not a
        // fan-out.
        for pair in group.windows(2) {
            if pair[0].input == pair[1].input {
                symbols.push(Step::Broadcast.symbol());
            } else {
                broken = true;
                notes.push(format!(
                    "{} and {} are declared parallel but read {} and {}",
                    pair[0].name, pair[1].name, pair[0].input, pair[1].input
                ));
                symbols.push(Step::Broken.symbol());
            }
        }
        // Between stages every upstream branch must feed every downstream one.
        if let Some(next) = shape.get(i + 1) {
            let mut ok = true;
            for a in group {
                for b in next {
                    if !a.outputs.iter().any(|out| compatible(out, &b.input)) {
                        ok = false;
                        notes.push(format!(
                            "{} yields {}, {} wants {}: does not compose",
                            a.name,
                            a.outputs
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join("|"),
                            b.name,
                            b.input
                        ));
                    }
                }
            }
            if ok {
                symbols.push(Step::Sequence.symbol());
            } else {
                broken = true;
                symbols.push(Step::Broken.symbol());
            }
        }
    }

    let flat: Vec<&Skill> = shape.iter().flatten().copied().collect();
    let has_merge = flat.iter().any(|s| s.input.as_str() == "M^n");
    if has_merge {
        symbols.push("⊕");
    }
    let has_iter = flat.iter().any(|s| iterates(s));
    if has_iter {
        symbols.push("*");
    }
    // Feedback: can every output of the last stage reach the first stage?
    // One branch suffices on either side — a loop closes if some path does.
    let repeat = match (shape.first(), shape.last()) {
        (Some(first), Some(last)) => last.iter().any(|l| {
            first
                .iter()
                .any(|f| l.outputs.iter().any(|out| compatible(out, &f.input)))
        }),
        _ => false,
    };
    if repeat {
        symbols.push("↺");
    }

    Factoring {
        chain: chain.to_string(),
        members,
        word: if symbols.is_empty() {
            "(single)".to_string()
        } else {
            symbols.join(" ")
        },
        stages: Stages {
            expand: symbols.contains(&"⊗"),
            execute: has_iter,
            select: has_merge,
            repeat,
        },
        typechecks: !broken,
        notes,
        shape: Some(
            shape
                .iter()
                .map(|g| g.iter().map(|s| s.name.clone()).collect())
                .collect(),
        ),
    }
}

/// Factor a factoring again, from what the algebra holds: the declared shape
/// when there is one, the member order otherwise. The round trip asks whether
/// the structure survived the rebuild, and a declared chain's structure is its
/// declaration — re-scanning its member list under the prose rule would ask a
/// different question and get a different word.
#[must_use]
pub fn refactor(f: &Factoring, by_name: &BTreeMap<String, Skill>) -> Factoring {
    match &f.shape {
        Some(shape) => {
            let tokens: Vec<String> = shape
                .iter()
                .map(|group| {
                    if group.len() == 1 {
                        group[0].clone()
                    } else {
                        format!("({})", group.join(" | "))
                    }
                })
                .collect();
            factor_declared(&f.chain, &tokens, by_name)
        }
        None => factor(&f.chain, &f.members, by_name),
    }
}

/// Factor every chain in a loaded corpus.
///
/// A chain is a skill whose name starts `chain:`. Its members come from its
/// `members:` declaration when it has one, and from the prose scan when it
/// does not. One naming no known member is skipped, as the oracle skips it: a
/// chain naming nothing analysable is not a counterexample to anything.
#[must_use]
pub fn factor_all(skills: &[Skill]) -> Vec<Factoring> {
    let by_name: BTreeMap<String, Skill> =
        skills.iter().map(|s| (s.name.clone(), s.clone())).collect();
    let known: Vec<String> = skills.iter().map(|s| s.name.clone()).collect();

    let mut out: Vec<Factoring> = skills
        .iter()
        .filter(|s| s.name.starts_with("chain:"))
        .filter_map(|skill| {
            if !skill.members.is_empty() {
                return Some(factor_declared(&skill.name, &skill.members, &by_name));
            }
            let text = std::fs::read_to_string(&skill.path).ok()?;
            let sequence = members(&text, &known);
            if sequence.is_empty() {
                return None;
            }
            Some(factor(&skill.name, &sequence, &by_name))
        })
        .collect();
    out.sort_by(|a, b| a.chain.cmp(&b.chain));
    out
}

// ─────────────────────────────────────────────────────────── rebuilding

/// Rebuild a factored chain as a [`Term`](pact_algebra::Term).
///
/// This is the "mechanically build them into the new program" half. The
/// factoring says what a chain *is*; this turns that back into something the
/// algebra holds, which is the only way to tell whether the decomposition
/// captured the chain or merely described it.
///
/// Consecutive members sharing an input become one `⊗`. An iterating effector
/// is wrapped in `*`. What remains is a `▸` sequence.
///
/// # Errors
///
/// The first type mismatch, named. A chain that does not typecheck cannot be
/// rebuilt, and that is the correct outcome rather than a limitation: the
/// algebra refuses to hold an ill-typed word, so a broken chain has no word to
/// hold. Ten of thirteen chains in the live corpus are in this state, and the
/// error names the same break the factoring did.
pub fn rebuild(
    factoring: &Factoring,
    by_name: &BTreeMap<String, Skill>,
) -> Result<pact_algebra::Term, pact_algebra::Mismatch> {
    use pact_algebra::Term;

    // A declared shape is the grouping. Otherwise group consecutive members
    // that read the same type: that grouping IS the fan-out under the scan's
    // rule — types decide, so a run of members sharing an input is one
    // parallel stage however the prose numbered them.
    let mut groups: Vec<Vec<&Skill>> = Vec::new();
    if let Some(shape) = &factoring.shape {
        for stage in shape {
            let group: Vec<&Skill> = stage.iter().filter_map(|n| by_name.get(n)).collect();
            if !group.is_empty() {
                groups.push(group);
            }
        }
    } else {
        for name in &factoring.members {
            let Some(skill) = by_name.get(name) else {
                continue;
            };
            match groups.last_mut() {
                Some(group) if group[0].input == skill.input => group.push(skill),
                _ => groups.push(vec![skill]),
            }
        }
    }

    let mut stages = Vec::new();
    for group in groups {
        let mut branches = Vec::new();
        for skill in &group {
            let atom = Term::atom(skill.name.clone(), skill.signature())?;
            // Alternative outputs widen the port set. Dropping them here would
            // break composition for a chain the oracle calls legal.
            let atom = skill
                .outputs
                .iter()
                .skip(1)
                .fold(atom, |t, ty| t.or_produces(ty.clone()));
            // Refinement widens it too: a specific material may be read as
            // the general one, which is what [`compatible`] let the factoring
            // accept. The algebra is nominal, so the port is added here rather
            // than taught to L1.
            let atom = if skill.outputs.iter().any(|t| t.as_str().starts_with("M_")) {
                atom.or_produces(Type::new("M"))
            } else {
                atom
            };
            branches.push(if iterates(skill) {
                atom.iterate()
            } else {
                atom
            });
        }
        stages.push(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Term::broadcast(branches)?
        });
    }

    Term::sequence(stages)
}

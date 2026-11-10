//! Tiered evaluation: engine, then model, then agent.
//!
//! Most questions about a corpus are structural. Does this chain typecheck.
//! Does this type have a producer. Is this pool under pressure. Every one of
//! those is decidable from signatures, and none of them should ever cost a
//! model call.
//!
//! # The failure this exists to prevent
//!
//! An evaluator that quietly calls a model for everything. It works, it is
//! slow, it is expensive, and — the part that matters — **it is no longer
//! reproducible**, because the answers stop being derivable from the corpus.
//! A structural question answered by a model is a structural fact turned into
//! an opinion.
//!
//! So escalation is recorded, not just permitted. Every question that leaves
//! the engine carries the reason it had to, and the report says what share of
//! the work the engine did alone. A number that drifts downward over releases
//! is the tier collapsing, and nothing else would show it.
//!
//! # Where the model actually is
//!
//! Nowhere, in this build, and that is stated rather than stubbed. `Escalated`
//! is what the engine produces when it cannot answer; wiring it to a local
//! model is a separate piece of work with its own gate. What exists here is
//! the **routing and the accounting**, which is the part that decides whether
//! a model is needed at all — and on the live corpus the answer is that it
//! almost never is.

use pact_corpus::{Factoring, Skill};

/// Who answered a question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Decided from structure. Free, exact, reproducible.
    Engine,
    /// Needs judgment about content a type cannot carry.
    Model,
    /// Needs to act: read a file, run something, look outside.
    Agent,
}

impl Tier {
    /// What this tier costs, in words.
    #[must_use]
    pub fn cost(self) -> &'static str {
        match self {
            Self::Engine => "free",
            Self::Model => "a call",
            Self::Agent => "a session",
        }
    }
}

/// One question, and who has to answer it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// What is being asked.
    pub about: String,
    /// Who can answer.
    pub tier: Tier,
    /// The answer, when the engine had one.
    pub answer: Option<String>,
    /// Why it escalated, when it did.
    ///
    /// Never empty for `Model` or `Agent`. An escalation with no reason is
    /// indistinguishable from a lazy default, and the whole point of the tier
    /// is being able to audit what left.
    pub reason: Option<String>,
}

/// What a run of questions cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Answered from structure.
    pub engine: usize,
    /// Escalated to a model.
    pub model: usize,
    /// Escalated to an agent.
    pub agent: usize,
}

impl Tally {
    /// How many questions in all.
    #[must_use]
    pub fn total(self) -> usize {
        self.engine + self.model + self.agent
    }

    /// Share the engine answered alone, as a percentage.
    ///
    /// The number to watch. Downward drift over releases is the tier
    /// collapsing, and nothing else in a report would show it.
    #[must_use]
    pub fn engine_share(self) -> u32 {
        if self.total() == 0 {
            return 100;
        }
        let engine = u32::try_from(self.engine).unwrap_or(u32::MAX);
        let total = u32::try_from(self.total()).unwrap_or(1);
        engine * 100 / total
    }
}

/// Ask everything the engine can decide about a corpus, and route the rest.
#[must_use]
pub fn interrogate(skills: &[Skill], factorings: &[Factoring]) -> Vec<Question> {
    let mut asked = Vec::new();

    for f in factorings {
        // Structural: does it compose. Types decide, so nothing escalates.
        asked.push(Question {
            about: format!("does {} typecheck", f.chain),
            tier: Tier::Engine,
            answer: Some(if f.typechecks { "yes" } else { "no" }.to_string()),
            reason: None,
        });
        asked.push(Question {
            about: format!("which stages does {} have", f.chain),
            tier: Tier::Engine,
            answer: Some(f.completeness()),
            reason: None,
        });

        // Not structural: whether the chain is any good. No type carries it
        // and no amount of analysis will produce it.
        asked.push(Question {
            about: format!("is {} worth running", f.chain),
            tier: Tier::Model,
            answer: None,
            reason: Some(
                "quality is not a property of a signature; determinism buys the \
                 predicates and never the judgment"
                    .to_string(),
            ),
        });
    }

    for skill in skills {
        asked.push(Question {
            about: format!("can {} ever fire", skill.name),
            tier: Tier::Engine,
            answer: Some(if skill.input.as_str() == "?" {
                "unknown: it declares no input type".to_string()
            } else {
                "yes".to_string()
            }),
            reason: None,
        });

        // Needs to look outside the frontmatter.
        if skill.category.is_none() {
            asked.push(Question {
                about: format!("what kind of thing is {}", skill.name),
                tier: Tier::Agent,
                answer: None,
                reason: Some("it declares no category; the body has to be read".to_string()),
            });
        }
    }

    asked
}

/// Count who answered what.
#[must_use]
pub fn tally(questions: &[Question]) -> Tally {
    let count = |tier: Tier| questions.iter().filter(|q| q.tier == tier).count();
    Tally {
        engine: count(Tier::Engine),
        model: count(Tier::Model),
        agent: count(Tier::Agent),
    }
}

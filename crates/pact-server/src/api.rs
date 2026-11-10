//! Handlers, one per view.
//!
//! # The corpus is read once, at boot
//!
//! Every endpoint reads the same in-memory snapshot. Re-reading 150 files per
//! request would be slower and, worse, would let two panels on one page
//! disagree because a file changed between their fetches. A corpus that
//! changes is handled by restarting, which is what the sync step does.
//!
//! # Not found is not empty
//!
//! `/api/skill/:name` returns 404 for a name that does not exist. An empty
//! body with a 200 would let the page render a detail view about nothing, and
//! the person reading it would conclude the skill is fine.

// Axum handlers are async by contract, whether or not they await anything.
// These do not: the corpus is in memory and every answer is a computation.
// Clippy is right that the async is unused and wrong that it is removable.
#![allow(clippy::unused_async)]

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use pact_corpus::Skill;
use std::sync::Arc;

use crate::view::{Chain, Chains, Graph, Overview, Run, SkillDetail};

/// Everything the handlers share: one corpus, read at boot.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// The skills, as loaded.
    pub skills: Vec<Skill>,
    /// Where they were read from, so the page can say.
    pub root: String,
}

/// The shared handle.
pub type Shared = Arc<Corpus>;

/// `GET /api/overview` — the health of the corpus.
pub async fn overview(State(corpus): State<Shared>) -> Json<Overview> {
    Json(Overview::of(&corpus.skills))
}

/// `GET /api/chains` — every chain, factored, with the corpus-wide tally.
pub async fn chains(State(corpus): State<Shared>) -> Json<Chains> {
    Json(Chains::of(&corpus.skills))
}

/// `GET /api/graph` — the type-flow diagram as data.
pub async fn graph(State(corpus): State<Shared>) -> Json<Graph> {
    Json(Graph::of(&corpus.skills))
}

/// `GET /api/tiers` — what the engine answered alone.
pub async fn tiers(State(corpus): State<Shared>) -> Json<Tiers> {
    let factorings = pact_corpus::factor_all(&corpus.skills);
    let questions = pact_cli_tiers::interrogate(&corpus.skills, &factorings);
    let tally = pact_cli_tiers::tally(&questions);

    let mut reasons: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for q in &questions {
        if q.tier != pact_cli_tiers::Tier::Engine {
            if let Some(reason) = &q.reason {
                *reasons.entry(reason.clone()).or_default() += 1;
            }
        }
    }

    Json(Tiers {
        total: tally.total(),
        engine: tally.engine,
        model: tally.model,
        agent: tally.agent,
        engine_share: tally.engine_share(),
        reasons: reasons.into_iter().collect(),
    })
}

/// What the engine answered alone, and why anything left.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Tiers {
    /// How many questions were asked.
    pub total: usize,
    /// Answered from structure.
    pub engine: usize,
    /// Escalated to a model.
    pub model: usize,
    /// Escalated to an agent.
    pub agent: usize,
    /// Share the engine answered alone.
    pub engine_share: u32,
    /// Why anything left, and how often.
    pub reasons: Vec<(String, usize)>,
}

/// `GET /api/trace` — the corpus, run.
///
/// Frames rather than raw events: a canvas needs how full each pool is and
/// what moved, and sending the events would push the aggregation into the
/// browser where it would be done differently from how the ledger does it.
pub async fn trace(State(corpus): State<Shared>) -> Json<Run> {
    Json(Run::of(&corpus.skills, 60, 1))
}

/// `GET /api/skill/:name` — one skill, or 404.
pub async fn skill(State(corpus): State<Shared>, Path(name): Path<String>) -> Response {
    match SkillDetail::of(&corpus.skills, &name) {
        Some(detail) => Json(detail).into_response(),
        // The whole point of the 404. See the module docs.
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "no such skill",
                "name": name,
            })),
        )
            .into_response(),
    }
}

/// `GET /api/meta` — where the corpus came from.
///
/// A page that shows numbers without saying what they are about is a page
/// somebody will eventually read against the wrong corpus.
pub async fn meta(State(corpus): State<Shared>) -> Json<Meta> {
    Json(Meta {
        root: corpus.root.clone(),
        skills: corpus.skills.len(),
    })
}

/// Where the numbers came from.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Meta {
    /// The corpus root, as given.
    pub root: String,
    /// How many typed skills were read from it.
    pub skills: usize,
}
/// Which chains to factor. Capped at 100 names per request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CheckChainsRequest {
    /// Chain names, answered in order.
    pub chains: Vec<String>,
}

/// One chain, factored — or not found.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChainCheck {
    /// The requested name, echoed.
    pub name: String,
    /// False when no chain by this name exists in the corpus.
    pub found: bool,
    /// The word, e.g. `⊗ ▸ * ▸ ⊕`. Absent when not found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
    /// How many of the four canonical stages it has. Absent when not found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub present: Option<usize>,
    /// Which stages are absent. Absent when not found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<Vec<String>>,
    /// Whether every consecutive pair composes. Absent when not found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typechecks: Option<bool>,
}

/// Batch answer, in request order.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CheckChainsResponse {
    /// One entry per requested name (capped at 100).
    pub results: Vec<ChainCheck>,
}

/// `POST /api/check-chains` — factor named chains against the live corpus.
///
/// Batch semantics: unknown names come back `found: false` in place, so one
/// bad name neither fails the batch nor shifts positions. Empty input yields
/// empty results. The corpus is the boot-loaded snapshot, like every view.
pub async fn check_chains(
    State(corpus): State<Shared>,
    Json(req): Json<CheckChainsRequest>,
) -> Json<CheckChainsResponse> {
    let factorings = pact_corpus::factor_all(&corpus.skills);
    let by_name: std::collections::BTreeMap<&str, _> =
        factorings.iter().map(|f| (f.chain.as_str(), f)).collect();
    let results = req
        .chains
        .iter()
        .take(100)
        .map(|name| match by_name.get(name.as_str()) {
            Some(f) => {
                let c = Chain::from(*f);
                ChainCheck {
                    name: name.clone(),
                    found: true,
                    word: Some(c.word),
                    present: Some(c.present),
                    missing: Some(c.missing),
                    typechecks: Some(c.typechecks),
                }
            }
            None => ChainCheck {
                name: name.clone(),
                found: false,
                word: None,
                present: None,
                missing: None,
                typechecks: None,
            },
        })
        .collect();
    Json(CheckChainsResponse { results })
}

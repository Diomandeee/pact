//! P6.2 — every endpoint answers, and 404 means something.
//!
//! The handlers are tested through `axum`'s own router rather than over a
//! socket. Binding a port in a test is a source of flake that says nothing
//! about whether the routing is right, and the thing under test is the routing
//! and the bodies.
//!
//! The one behaviour worth stating: **an unknown skill is a 404, not a 200
//! with an empty object.** "No findings" and "no such thing" are different
//! answers, and a page that renders a detail view about nothing lets the
//! person reading it conclude the skill is fine.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::Router;
use pact_corpus::{default_root, load};
use pact_server::{api, Chains, Graph, Overview, SkillDetail};
use std::sync::Arc;
use tower::ServiceExt;

fn app() -> Option<(Router, Vec<pact_corpus::Skill>)> {
    let skills = load(&default_root()).ok()?;
    if skills.is_empty() {
        return None;
    }
    let corpus: api::Shared = Arc::new(api::Corpus {
        skills: skills.clone(),
        root: default_root().display().to_string(),
    });
    let router = Router::new()
        .route("/health", get(api::meta))
        .route("/api/meta", get(api::meta))
        .route("/api/overview", get(api::overview))
        .route("/api/chains", get(api::chains))
        .route("/api/check-chains", post(api::check_chains))
        .route("/api/graph", get(api::graph))
        .route("/api/tiers", get(api::tiers))
        .route("/api/skill/{name}", get(api::skill))
        .with_state(corpus);
    Some((router, skills))
}

async fn get_json(router: &Router, path: &str) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn every_endpoint_answers_with_what_the_analysis_computed() {
    let Some((router, skills)) = app() else {
        println!("SKIPPED: no corpus");
        return;
    };

    let (status, body) = get_json(&router, "/api/overview").await;
    assert_eq!(status, StatusCode::OK);
    let served: Overview = serde_json::from_str(&body).expect("overview parses");
    assert_eq!(
        served,
        Overview::of(&skills),
        "the endpoint and the view disagree"
    );

    let (status, body) = get_json(&router, "/api/chains").await;
    assert_eq!(status, StatusCode::OK);
    let served: Chains = serde_json::from_str(&body).expect("chains parse");
    assert_eq!(served, Chains::of(&skills));

    let (status, body) = get_json(&router, "/api/graph").await;
    assert_eq!(status, StatusCode::OK);
    let served: Graph = serde_json::from_str(&body).expect("graph parses");
    assert_eq!(served, Graph::of(&skills));

    let (status, body) = get_json(&router, "/api/tiers").await;
    assert_eq!(status, StatusCode::OK);
    let served: api::Tiers = serde_json::from_str(&body).expect("tiers parse");
    assert!(
        served.engine_share >= 80,
        "the engine answered {}% and the floor is 80",
        served.engine_share
    );
    assert_eq!(served.total, served.engine + served.model + served.agent);
}

#[tokio::test]
async fn a_known_skill_returns_its_detail() {
    let Some((router, skills)) = app() else {
        return;
    };
    let name = &skills[0].name;
    let (status, body) = get_json(&router, &format!("/api/skill/{name}")).await;

    assert_eq!(status, StatusCode::OK);
    let served: SkillDetail = serde_json::from_str(&body).expect("detail parses");
    assert_eq!(served, SkillDetail::of(&skills, name).expect("it exists"));
}

#[tokio::test]
async fn an_unknown_skill_is_a_404_and_not_an_empty_object() {
    // The behaviour this endpoint exists to get right.
    let Some((router, _)) = app() else {
        return;
    };
    let (status, body) = get_json(&router, "/api/skill/no-such-skill-anywhere").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.contains("no such skill"),
        "the body must say which kind of nothing this is: {body}"
    );
    assert!(
        serde_json::from_str::<SkillDetail>(&body).is_err(),
        "a 404 body must not parse as a detail; that is how an empty panel \
         becomes a claim about a skill"
    );
}

#[tokio::test]
async fn the_page_says_which_corpus_it_read() {
    // A page showing numbers without saying what they are about is one
    // somebody eventually reads against the wrong corpus.
    let Some((router, skills)) = app() else {
        return;
    };
    let (status, body) = get_json(&router, "/api/meta").await;
    assert_eq!(status, StatusCode::OK);

    let served: api::Meta = serde_json::from_str(&body).expect("meta parses");
    assert_eq!(served.skills, skills.len());
    assert!(!served.root.is_empty());
}
async fn post_json(router: &Router, path: &str, body: String) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(path)
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("the router answers");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a body");
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn check_chains_reports_found_and_missing_in_place() {
    let Some((router, skills)) = app() else {
        println!("SKIPPED: no corpus");
        return;
    };
    let known = Chains::of(&skills)
        .chains
        .into_iter()
        .next()
        .map(|c| c.name);
    let Some(known) = known else {
        println!("SKIPPED: corpus has no chains");
        return;
    };
    // Batch: known + unknown, order preserved.
    let body =
        serde_json::json!({ "chains": [known.clone(), "no-such-chain-anywhere"] }).to_string();
    let (status, text) = post_json(&router, "/api/check-chains", body).await;
    assert_eq!(status, StatusCode::OK);
    let parsed: api::CheckChainsResponse = serde_json::from_str(&text).expect("typed body");
    assert_eq!(parsed.results.len(), 2);
    assert_eq!(parsed.results[0].name, known);
    assert!(parsed.results[0].found);
    let present = parsed.results[0].present.expect("present");
    let missing = parsed.results[0].missing.as_ref().expect("missing");
    assert_eq!(present + missing.len(), 4, "stages partition");
    assert!(parsed.results[0].typechecks.is_some());
    for m in missing {
        assert!(
            ["⊗ expand", "* execute", "⊕ select", "↺ repeat"].contains(&m.as_str()),
            "canonical stage label: {m}"
        );
    }
    assert_eq!(parsed.results[1].name, "no-such-chain-anywhere");
    assert!(!parsed.results[1].found);
    assert!(parsed.results[1].present.is_none());
    // Empty batch yields empty results, not an error.
    let (status, text) =
        post_json(&router, "/api/check-chains", r#"{"chains":[]}"#.to_string()).await;
    assert_eq!(status, StatusCode::OK);
    let parsed: api::CheckChainsResponse = serde_json::from_str(&text).expect("typed body");
    assert!(parsed.results.is_empty());
    // Batch cap: 101 names in, 100 results out.
    let many: Vec<String> = (0..101).map(|i| format!("chain-{i}")).collect();
    let body = serde_json::json!({ "chains": many }).to_string();
    let (status, text) = post_json(&router, "/api/check-chains", body).await;
    assert_eq!(status, StatusCode::OK);
    let parsed: api::CheckChainsResponse = serde_json::from_str(&text).expect("typed body");
    assert_eq!(parsed.results.len(), 100);
}

#[tokio::test]
async fn health_is_meta_and_nothing_else_is_health() {
    // Service monitors probe `/health` on every port a service declares.
    // It must be the same object as `/api/meta` — a count that moved is the
    // signal — and a spelling the registry does not use must stay a 404, or a
    // probe that typo'd its way to 200 would report a dead server alive.
    let Some((router, skills)) = app() else {
        println!("SKIPPED: no corpus");
        return;
    };
    let (status, health) = get_json(&router, "/health").await;
    assert_eq!(status, StatusCode::OK);
    let (_, meta) = get_json(&router, "/api/meta").await;
    assert_eq!(health, meta, "/health and /api/meta drifted apart");
    let served: api::Meta = serde_json::from_str(&health).expect("health parses");
    assert_eq!(served.skills, skills.len());

    let (status, _) = get_json(&router, "/healthz").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "/healthz is not a route");
}

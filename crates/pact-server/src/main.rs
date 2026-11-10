//! `pact-server` — one corpus, over HTTP, with a page.
//!
//! # Binding
//!
//! Defaults to `127.0.0.1`. To serve the tailnet, pass an address:
//!
//! ```text
//! pact-server                          # localhost only
//! pact-server 0.0.0.0:8730             # every interface
//! pact-server 100.x.y.z:8730 /srv/skills
//! ```
//!
//! Binding to `0.0.0.0` on a machine with a public interface exposes this to
//! the internet, and there is no auth. P6.5's gate is that it answers over
//! Tailscale and does **not** answer publicly; the second half is the one that
//! matters, and it is a firewall question rather than a code question.
//!
//! # The corpus is read once
//!
//! At boot, into memory. Re-reading per request would be slower and would let
//! two panels on one page disagree because a file changed between their
//! fetches.

use axum::extract::State;
use axum::routing::{get, post};
use axum::Router;
use pact_server::api;
use std::net::SocketAddr;
use std::sync::Arc;

const INDEX: &str = include_str!("../web/index.html");
const STYLE: &str = include_str!("../web/app.css");
const SCRIPT: &str = include_str!("../web/app.js");

// The instrument. Bundled by `deno bundle` from app/src -- one binary, no npm
// -- and the .js is checked in so the server needs no toolchain to serve it.
const APP_HTML: &str = include_str!("../../../app/index.html");
const APP_CSS: &str = include_str!("../../../app/app.css");
const APP_JS: &str = include_str!("../../../app/main.js");
const APP_WASM: &[u8] =
    include_bytes!("../../../target/wasm32-unknown-unknown/release/pact_wasm.wasm");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let bind: SocketAddr = args
        .next()
        .unwrap_or_else(|| "127.0.0.1:8730".to_string())
        .parse()?;
    let root = args
        .next()
        .map_or_else(pact_corpus::default_root, std::path::PathBuf::from);

    let skills = pact_corpus::load(&root)?;
    if skills.is_empty() {
        // The same refusal the CLI makes, for the same reason: "0 skills,
        // everything fine" for a path that is wrong looks like a result.
        eprintln!("no typed skills under {}", root.display());
        eprintln!("  a file counts when it carries a SOOP-2 block");
        std::process::exit(1);
    }
    let count = skills.len();

    let corpus: api::Shared = Arc::new(api::Corpus {
        skills,
        root: root.display().to_string(),
    });

    let app = Router::new()
        // Server-rendered headline. P6.3's gate is that the numbers are in
        // the HTML before any JavaScript runs, and a page whose first paint is
        // a row of em-dashes is one where a slow fetch and a broken corpus
        // look identical.
        .route("/", get(index))
        .route(
            "/app.css",
            get(|| async { ([("content-type", "text/css")], STYLE) }),
        )
        .route(
            "/app.js",
            get(|| async { ([("content-type", "text/javascript")], SCRIPT) }),
        )
        .route("/app", get(|| async { axum::response::Html(APP_HTML) }))
        .route(
            "/app/app.css",
            get(|| async { ([("content-type", "text/css")], APP_CSS) }),
        )
        .route(
            "/app/main.js",
            get(|| async { ([("content-type", "text/javascript")], APP_JS) }),
        )
        .route(
            "/pact_wasm.wasm",
            get(|| async { ([("content-type", "application/wasm")], APP_WASM) }),
        )
        // Service monitors probe /health on every declared port. It is the
        // same object as /api/meta: a count that moved is the health signal.
        .route("/health", get(api::meta))
        .route("/api/meta", get(api::meta))
        .route("/api/overview", get(api::overview))
        .route("/api/chains", get(api::chains))
        .route("/api/check-chains", post(api::check_chains))
        .route("/api/graph", get(api::graph))
        .route("/api/tiers", get(api::tiers))
        .route("/api/trace", get(api::trace))
        .route("/api/skill/{name}", get(api::skill))
        .with_state(corpus);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("pact — {count} skills from {}", root.display());
    eprintln!("       http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// The page, with its headline numbers already in it.
///
/// Five substitutions, no template engine. A dependency whose job is replacing
/// five strings would be the largest thing in this crate after axum.
#[allow(clippy::unused_async)]
async fn index(State(corpus): State<api::Shared>) -> axum::response::Html<String> {
    let overview = pact_server::Overview::of(&corpus.skills);
    let factorings = pact_corpus::factor_all(&corpus.skills);
    let questions = pact_cli_tiers::interrogate(&corpus.skills, &factorings);
    let share = pact_cli_tiers::tally(&questions).engine_share();

    let page = INDEX
        .replace("{{skills}}", &overview.skills.to_string())
        .replace("{{types}}", &overview.types.to_string())
        .replace("{{loops}}", &overview.feedback_loops.to_string())
        .replace("{{dead}}", &overview.never_fires.len().to_string())
        .replace("{{share}}", &format!("{share}%"))
        .replace("{{verdict}}", &escape(&overview.verdict))
        .replace(
            "{{root}}",
            &escape(&format!("{} skills · {}", corpus.skills.len(), corpus.root)),
        );
    axum::response::Html(page)
}

/// Minimal HTML escaping for the substituted values.
///
/// A corpus root is a path and a verdict is generated prose; neither is
/// hostile, and neither is guaranteed not to contain an angle bracket.
fn escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

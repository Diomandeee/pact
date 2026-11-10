//! P7.2 — the generated TypeScript typechecks.
//!
//! The generation itself is a unit test in `view.rs`, because
//! `cfg_attr(test, ...)` applies only when that crate is compiled with
//! `--test`; an integration test links the ordinary build where the derives
//! are not there. This half is the gate: **renaming a field in Rust must break
//! the TypeScript build.**
//!
//! `deno check` is the whole toolchain. One binary, already installed, no npm
//! and no `node_modules` — which is the same answer this project has given
//! every time a build step was proposed.

use pact_corpus::{default_root, load};
use std::path::{Path, PathBuf};
use std::process::Command;

fn app() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .join("app/src")
}

#[test]
fn the_generated_types_exist_and_use_the_wire_names() {
    let generated = app().join("types.ts");
    if !generated.exists() {
        println!("SKIPPED: run `cargo test -p pact-server --lib` to generate them");
        return;
    }
    let text = std::fs::read_to_string(&generated).expect("readable");

    // Every key serde actually emits for an Overview must be named. This is
    // the specific failure a hand-kept mirror produces: the server sends
    // `never_fires`, the page reads something else, the panel is blank and
    // nothing anywhere reports an error.
    let Ok(skills) = load(&default_root()) else {
        return;
    };
    if skills.is_empty() {
        return;
    }
    let json = serde_json::to_string(&pact_server::Overview::of(&skills)).expect("serialises");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parses");
    for key in value.as_object().expect("an object").keys() {
        assert!(
            text.contains(key),
            "serde sends `{key}` and the generated TypeScript does not mention it"
        );
    }
}

#[test]
fn the_typescript_typechecks() {
    // The gate. If this stops passing because a field was renamed in Rust,
    // that is the mechanism working rather than a nuisance.
    let entry = app().join("engine.ts");
    if !entry.exists() {
        println!("SKIPPED: app/src/engine.ts does not exist yet");
        return;
    }
    let Ok(out) = Command::new("deno")
        .args(["check", "--quiet"])
        .arg(&entry)
        .output()
    else {
        println!("SKIPPED: deno is not installed");
        return;
    };
    assert!(
        out.status.success(),
        "the TypeScript does not typecheck against the generated types:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

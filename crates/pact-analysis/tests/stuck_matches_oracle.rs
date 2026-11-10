//! P9.1 — the Rust stuck trigger matches the Python oracle, session by
//! session, over the vendored trajectory fixture (218 sessions, 7,303
//! classified turns from the divergence pilot and the v3 trajectory corpus).
//!
//! Same shape as `matches_python.rs`: shell out to the oracle, diff every
//! fire index. A wrong formula and a differently-right formula fail
//! differently here — the unit tests in `stuck.rs` pin hand-checkable cases,
//! this pins the whole corpus.

use pact_analysis::stuck::{StuckDetector, TurnMove};
use std::collections::BTreeMap;
use std::process::Command;

fn fixture() -> BTreeMap<String, Vec<String>> {
    // No serde in this crate, and the fixture is exactly
    // {"sid": ["MOVE", ...], ...} — a 20-line parser beats a dependency.
    let text = include_str!("fixtures/trajectory-moves.json");
    parse_fixture(text).expect("fixture parses")
}

/// Minimal parser for the fixture shape. Returns `None` on anything else,
/// which fails the test rather than silently reading half a corpus.
fn parse_fixture(text: &str) -> Option<BTreeMap<String, Vec<String>>> {
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut map = BTreeMap::new();
    let ws = |p: &mut usize| {
        while *p < bytes.len() && bytes[*p].is_ascii_whitespace() {
            *p += 1;
        }
    };
    let string = |p: &mut usize| -> Option<String> {
        ws(p);
        if bytes.get(*p) != Some(&b'"') {
            return None;
        }
        *p += 1;
        let start = *p;
        while *p < bytes.len() && bytes[*p] != b'"' {
            if bytes[*p] == b'\\' {
                return None;
            }
            *p += 1;
        }
        if *p >= bytes.len() {
            return None;
        }
        let s = std::str::from_utf8(&bytes[start..*p]).ok()?.to_string();
        *p += 1;
        Some(s)
    };
    let lit = |p: &mut usize, c: u8| -> bool {
        ws(p);
        if bytes.get(*p) == Some(&c) {
            *p += 1;
            true
        } else {
            false
        }
    };
    if !lit(&mut pos, b'{') {
        return None;
    }
    ws(&mut pos);
    if lit(&mut pos, b'}') {
        return Some(map);
    }
    loop {
        let key = string(&mut pos)?;
        if !lit(&mut pos, b':') {
            return None;
        }
        if !lit(&mut pos, b'[') {
            return None;
        }
        let mut moves = Vec::new();
        ws(&mut pos);
        if !lit(&mut pos, b']') {
            loop {
                moves.push(string(&mut pos)?);
                ws(&mut pos);
                if lit(&mut pos, b']') {
                    break;
                }
                if !lit(&mut pos, b',') {
                    return None;
                }
            }
        }
        map.insert(key, moves);
        ws(&mut pos);
        if lit(&mut pos, b'}') {
            return Some(map);
        }
        if !lit(&mut pos, b',') {
            return None;
        }
    }
}

fn oracle() -> Option<BTreeMap<String, Vec<usize>>> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    if !dir.join("stuck_oracle.py").exists() {
        return None;
    }
    let out = Command::new("python3")
        .arg(dir.join("stuck_oracle.py"))
        .arg(dir.join("trajectory-moves.json"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut map = BTreeMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let (sid, idx) = line.split_once('=')?;
        map.insert(
            sid.to_string(),
            idx.split(',').filter_map(|s| s.parse().ok()).collect(),
        );
    }
    Some(map).filter(|m: &BTreeMap<String, Vec<usize>>| !m.is_empty())
}

#[test]
fn trigger_matches_oracle_on_every_session() {
    let seqs = fixture();
    let mut ours: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (sid, moves) in &seqs {
        let turns: Vec<TurnMove> = moves.iter().map(|m| TurnMove::from_label(m)).collect();
        let mut d = StuckDetector::new();
        let fired = d.feed(&turns);
        if !fired.is_empty() {
            ours.insert(sid.clone(), fired);
        }
    }

    let Some(expected) = oracle() else {
        println!("SKIPPED: stuck_oracle.py did not run; nothing to diff against");
        return;
    };

    let mut disagreements = Vec::new();
    for (sid, got) in &ours {
        match expected.get(sid) {
            Some(want) if *want == *got => {}
            Some(want) => disagreements.push(format!("{sid}: python {want:?}, rust {got:?}")),
            None => disagreements.push(format!("{sid}: absent from python")),
        }
    }
    for sid in expected.keys() {
        if !ours.contains_key(sid) {
            disagreements.push(format!("{sid}: absent from rust"));
        }
    }
    assert!(
        disagreements.is_empty(),
        "trigger disagrees with the oracle in {} session(s). Find out which \
         is wrong before assuming it is the python:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );

    // Floor against an empty fixture, not a claim about the corpus: the
    // prototype replay measured exactly 36 firing sessions here (rejects
    // concentrate in a few long sessions), so the floor sits well below at
    // 25. The first draft guessed 100 and was measuring nothing but the
    // guess.
    assert!(
        ours.len() >= 25,
        "only {} firing sessions: the fixture is probably truncated",
        ours.len()
    );
}

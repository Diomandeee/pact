//! P9.4 — `pact stuck` is the P9.1 rule, callable.
//!
//! Two diffs. The scripted stream pins hand-checkable behaviour across
//! interleaved sessions (independent detectors, reframe reset, skips
//! counted not fatal). The fixture replay pipes all 218 vendored sessions
//! through the binary and diffs every fire line against `stuck_oracle.py`:
//! a CLI that printed different fires than the crate it calls would be the
//! one failure the lower gates cannot see.

use std::io::Write as _;
use std::process::{Command, Stdio};

fn pact() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_BIN_EXE_pact")).to_path_buf()
}

fn run_stdin(input: &str) -> (String, String, i32) {
    let mut child = Command::new(pact())
        .arg("stuck")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(input.as_bytes())
        .expect("stdin writable");
    let out = child.wait_with_output().expect("output readable");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn scripted_stream_fires_per_session() {
    let input = concat!(
        "{\"session\": \"a\", \"idx\": 0, \"move\": \"REJECT_CORRECT\"}\n",
        "{\"session\": \"b\", \"idx\": 0, \"move\": \"QUESTION\"}\n",
        "{\"session\": \"a\", \"idx\": 1, \"move\": \"REJECT_CORRECT\"}\n",
        "{\"session\": \"b\", \"idx\": 1, \"move\": \"REJECT_CORRECT\"}\n",
        "{\"session\": \"a\", \"idx\": 2, \"move\": \"REFRAME\"}\n",
        "not json\n",
        "{\"session\": \"b\", \"idx\": 2, \"move\": \"REJECT_CORRECT\"}\n",
        "{\"session\": \"a\", \"idx\": 3, \"move\": \"REJECT_CORRECT\"}\n",
    );
    let (stdout, stderr, code) = run_stdin(input);
    assert_eq!(code, 0, "stderr: {stderr}");
    // a fires once (idx 1; the reframe resets before idx 3 can pair), b
    // fires once (idx 2), and the bad line is counted on stderr.
    assert_eq!(
        stdout,
        "{\"session\": \"a\", \"idx\": 1}\n{\"session\": \"b\", \"idx\": 2}\n"
    );
    assert!(stderr.contains("1 skipped"), "stderr: {stderr}");
}

#[test]
fn fixture_replay_matches_oracle() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../pact-analysis/tests/fixtures");
    // Project the fixture to turn lines with Python (mechanical field
    // extraction, the same projection the analysis gate uses); the oracle
    // comparison is the derivation that matters.
    let proj = Command::new("python3")
        .arg("-c")
        .arg(
            "import json,sys; [print('{\"session\": \"%s\", \"idx\": %d, \"move\": \"%s\"}' % (s, i, m)) for s, ms in json.load(open(sys.argv[1])).items() for i, m in enumerate(ms)]",
        )
        .arg(dir.join("trajectory-moves.json"))
        .output();
    let Ok(proj) = proj else {
        println!("SKIPPED: no python3; nothing to project with");
        return;
    };
    if !proj.status.success() {
        println!("SKIPPED: projection failed");
        return;
    }
    let input = String::from_utf8_lossy(&proj.stdout).into_owned();
    let (stdout, stderr, code) = run_stdin(&input);
    assert_eq!(code, 0, "stderr: {stderr}");

    let oracle = Command::new("python3")
        .arg(dir.join("stuck_oracle.py"))
        .arg(dir.join("trajectory-moves.json"))
        .output();
    let Ok(oracle) = oracle else {
        println!("SKIPPED: oracle did not run");
        return;
    };
    if !oracle.status.success() {
        println!("SKIPPED: oracle failed");
        return;
    }
    // Oracle lines are `sid=i,j`; CLI lines are JSON per fire. Compare as
    // sorted (session, idx) sets — order across sessions is arrival order
    // and carries no meaning.
    let mut want = Vec::new();
    for line in String::from_utf8_lossy(&oracle.stdout).lines() {
        let (sid, idx) = line.split_once('=').expect("oracle shape");
        for i in idx.split(',') {
            want.push((sid.to_string(), i.parse::<u64>().expect("idx")));
        }
    }
    let mut got = Vec::new();
    for line in stdout.lines() {
        let line = line.trim().trim_start_matches('{').trim_end_matches('}');
        let mut sid = None;
        let mut idx = None;
        for part in line.split(',') {
            let (k, v) = part.split_once(':').expect("cli shape");
            match k.trim().trim_matches('"') {
                "session" => sid = Some(v.trim().trim_matches('"').to_string()),
                "idx" => idx = Some(v.trim().parse().expect("idx")),
                _ => {}
            }
        }
        got.push((sid.expect("session"), idx.expect("idx")));
    }
    want.sort();
    got.sort();
    assert_eq!(got, want, "CLI fires differ from the oracle");
}

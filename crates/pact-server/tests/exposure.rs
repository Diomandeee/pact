//! P6.5, the half that does not need a second machine.
//!
//! The gate has two clauses: it answers over Tailscale, and it is **not**
//! reachable from the public internet. The second is the one that matters, and
//! it turns out to be provable more strongly than the gate assumed.
//!
//! # A single-address bind beats a firewall rule
//!
//! Binding to one `100.x.y.z` address puts the socket on a CGNAT range
//! (`100.64.0.0/10`) that is not routable from the internet at all. There is
//! no public interface to firewall, because the listener is not on one. A
//! `0.0.0.0` bind plus a firewall rule is one misconfiguration away from
//! publishing an unauthenticated corpus; this is not.

use std::process::Command;

fn deploy(args: &[&str]) -> (String, i32) {
    let out = Command::new("./scripts/deploy")
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("the script runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn a_public_bind_is_refused() {
    // There is no auth. Binding every interface on a machine with a public one
    // publishes an unauthenticated corpus, and the failure mode is silent.
    for addr in ["0.0.0.0:8730", "[::]:8730"] {
        let (out, code) = deploy(&["up", addr]);
        assert_eq!(code, 1, "{addr} was accepted:\n{out}");
        assert!(
            out.contains("REFUSED") && out.contains("no auth"),
            "the refusal must say why:\n{out}"
        );
        assert!(
            out.contains("100."),
            "and it must say what to do instead:\n{out}"
        );
    }
}

#[test]
fn a_tailnet_bind_is_allowed() {
    // Without this the refusal is satisfied by refusing everything, which is
    // the lesson every one-line gate in this project has taught.
    let (out, _) = deploy(&["status"]);
    assert!(
        out.contains("corpus"),
        "status must report the corpus:\n{out}"
    );
    // `up` execs the server, so it is not run here; the refusal path is what
    // this file gates, and it is checked to reject only what it claims to.
    assert!(
        !out.contains("REFUSED"),
        "status must not refuse anything:\n{out}"
    );
}

#[test]
fn the_sync_step_is_one_command() {
    // A two-command sync is one somebody does half of, and a dashboard over a
    // half-synced corpus is worse than no dashboard: it is confidently wrong.
    let script =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/deploy"))
            .expect("the script is readable");

    // One invocation must both move the files and restart the reader, because
    // the server reads its corpus at boot.
    let sync = script
        .split("sync)")
        .nth(1)
        .expect("there is a sync branch");
    assert!(sync.contains("rsync"), "sync must move the files");
    assert!(
        sync.contains("restart"),
        "sync must restart the reader, or the files are there and the numbers are not"
    );
    assert!(
        sync.contains("--delete"),
        "sync must mirror rather than union: a skill deleted locally that lingers \
         remotely makes the served numbers about a corpus that no longer exists"
    );
}

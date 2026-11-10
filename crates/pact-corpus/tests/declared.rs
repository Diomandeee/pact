//! P3.4 — declared membership, declared iteration, and refinement.
//!
//! Three things the frontmatter could not say before 2026-11-03, each with
//! both directions checked: the positive case, the negative case, and the
//! shape that would let a lazy implementation pass.
//!
//! - `members: [a, b]` on a chain beats the prose scan. The negative case is a
//!   chain that declares nothing, which must still be scanned exactly as the
//!   oracle scans it. The circular case is a declared member that does not
//!   exist: it must be noted and must break the chain, or a chain could
//!   declare its way to legality.
//! - `iterates: true` puts `*` in the word without the name being on the
//!   fallback list. The negative case is a skill that declares nothing and is
//!   not on the list.
//! - Refinement: `M_creative` satisfies `M`, `M^n` does not, and the general
//!   never satisfies the specific.

use pact_core::Type;
use pact_corpus::{compatible, factor, factor_declared, iterates, rebuild, stages, Skill, Stage};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn skill(name: &str, inp: &str, out: &str) -> Skill {
    Skill {
        name: name.to_string(),
        input: Type::new(inp),
        outputs: out.split('|').map(|t| Type::new(t.trim())).collect(),
        category: None,
        effects: None,
        idempotent: false,
        commutes_with: vec![],
        members: vec![],
        iterates: false,
        path: PathBuf::new(),
    }
}

fn corpus(skills: Vec<Skill>) -> BTreeMap<String, Skill> {
    skills.into_iter().map(|s| (s.name.clone(), s)).collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn the_frontmatter_carries_members_and_iterates() {
    let text = "---\nname: chain:x\n# === SOOP-2 type system (v1) ===\ncategory: Effector\n\
                input_type: M_plan\noutput_type: State\nmembers: [pulse, compact, divergent-rail]\n\
                iterates: true\ntype_spec_version: 1\n---\n\nbody mentions merge and chain\n";
    let s = Skill::parse(Path::new("/corpus/chain:x/SKILL.md"), text).expect("typed");
    assert_eq!(s.members, names(&["pulse", "compact", "divergent-rail"]));
    assert!(s.iterates);

    // Negative: nothing declared reads as nothing declared, not as a default
    // that happens to look right.
    let text = "---\nname: y\ncategory: Generator\ninput_type: Subject\noutput_type: M\n\
                type_spec_version: 1\n---\n";
    let s = Skill::parse(Path::new("/corpus/y/SKILL.md"), text).expect("typed");
    assert!(s.members.is_empty());
    assert!(!s.iterates);

    // `iterates: false` and `iterates: yes` are both not true. Only the
    // literal is a declaration.
    let text =
        "---\nname: z\ninput_type: M\noutput_type: M\niterates: yes\ntype_spec_version: 1\n---\n";
    let s = Skill::parse(Path::new("/corpus/z/SKILL.md"), text).expect("typed");
    assert!(!s.iterates, "`yes` is not a declaration");
}

#[test]
fn a_declared_member_list_is_the_word() {
    let by_name = corpus(vec![
        skill("rail", "M", "M_plan"),
        skill("run", "M_plan", "State"),
        skill("compact", "State", "M_arch"),
    ]);
    // Declared in the loop's own order: run first, so the word closes.
    let f = factor_declared("chain:loop", &names(&["run", "compact", "rail"]), &by_name);
    assert_eq!(f.members, names(&["run", "compact", "rail"]));
    assert_eq!(f.word, "▸ ▸ ↺", "{:?}", f.notes);
    assert!(f.typechecks);
    assert!(f.stages.repeat);
}

#[test]
fn a_declared_member_that_does_not_exist_breaks_the_chain() {
    let by_name = corpus(vec![
        skill("rail", "M", "M_plan"),
        skill("run", "M_plan", "State"),
    ]);
    let f = factor_declared("chain:ghost", &names(&["rail", "ghost", "run"]), &by_name);
    // The ghost is out of the word — the two real members still compose —
    // but the chain does not typecheck and the note says why.
    assert_eq!(f.members, names(&["rail", "run"]));
    assert_eq!(f.word, "▸");
    assert!(!f.typechecks, "a declaration naming nothing must not pass");
    assert!(
        f.notes
            .iter()
            .any(|n| n.contains("ghost") && n.contains("not a typed skill")),
        "{:?}",
        f.notes
    );

    // Positive control on the same corpus: the honest declaration passes.
    let f = factor_declared("chain:real", &names(&["rail", "run"]), &by_name);
    assert!(f.typechecks);
    assert!(f.notes.is_empty());
}

#[test]
fn iteration_is_declared_not_listed() {
    let mut looper = skill("looper", "M_plan", "State");
    looper.iterates = true;
    let by_name = corpus(vec![
        skill("rail", "M", "M_plan"),
        looper,
        skill("plain", "M_plan", "State"),
    ]);
    assert!(iterates(by_name.get("looper").expect("present")));
    assert!(!iterates(by_name.get("plain").expect("present")));

    let f = factor("chain:a", &names(&["rail", "looper"]), &by_name);
    assert_eq!(f.word, "▸ *");
    assert!(f.stages.execute);

    // Negative: an undeclared skill off the fallback list does not iterate,
    // however much its name suggests it.
    let f = factor("chain:b", &names(&["rail", "plain"]), &by_name);
    assert_eq!(f.word, "▸");
    assert!(!f.stages.execute);

    // The fallback list still speaks for a corpus that declared nothing.
    let by_name = corpus(vec![
        skill("rail", "M", "M_plan"),
        skill("pulse", "M_plan", "State"),
    ]);
    let f = factor("chain:c", &names(&["rail", "pulse"]), &by_name);
    assert!(f.stages.execute, "pulse is on the fallback list");
}

#[test]
fn refinement_reads_the_specific_as_the_general_and_never_the_reverse() {
    assert!(compatible(&Type::new("M_creative"), &Type::new("M")));
    assert!(compatible(&Type::new("M_review"), &Type::new("M")));
    assert!(compatible(&Type::new("M"), &Type::new("M")));
    assert!(
        !compatible(&Type::new("M"), &Type::new("M_plan")),
        "no downcast"
    );
    assert!(
        !compatible(&Type::new("M^n"), &Type::new("M")),
        "a compound is not a material"
    );
    assert!(!compatible(&Type::new("Subject"), &Type::new("M")));
    assert!(!compatible(&Type::new("State"), &Type::new("M")));
    assert!(
        !compatible(&Type::new("M_plan"), &Type::new("M_arch")),
        "siblings do not refine each other"
    );

    let by_name = corpus(vec![
        skill("evolve", "M", "M_creative"),
        skill("review", "M", "M_review"),
        skill("forge", "M", "M_arch"),
        skill("gather", "State", "M^n"),
    ]);
    // Declared as a sequence. The scan's rule would call three readers of M
    // parallel; the declaration says review reads what evolve made, and the
    // types agree via refinement.
    let f = factor_declared(
        "chain:refined",
        &names(&["evolve", "review", "forge"]),
        &by_name,
    );
    assert_eq!(f.word, "▸ ▸ ↺", "{:?}", f.notes);
    assert!(f.typechecks);
    // And the algebra holds the same word: the port set is widened at rebuild
    // exactly as far as the factoring widened it, no further.
    let term = rebuild(&f, &by_name).expect("a refined chain rebuilds");
    assert_eq!(term.ports().input, Type::new("M"));

    let f = factor_declared("chain:compound", &names(&["gather", "forge"]), &by_name);
    assert_eq!(f.word, "✗", "M^n must not pass as M");
    assert!(!f.typechecks);
    assert!(rebuild(&f, &by_name).is_err());
}

#[test]
fn a_declared_fan_out_is_a_fan_out_and_a_declared_sequence_is_a_sequence() {
    // `(a | b)` is one stage. The same two names declared in sequence are two
    // stages, and then the types must compose or the chain is broken — even
    // though under the scan's rule two readers of M_plan would have been
    // silently reclassified as parallel.
    assert_eq!(
        stages(&names(&["rail", "(pulse | ops:autopilot)", "merge"])),
        vec![
            Stage::One("rail".into()),
            Stage::Many(vec!["pulse".into(), "ops:autopilot".into()]),
            Stage::One("merge".into()),
        ]
    );

    let by_name = corpus(vec![
        skill("rail", "M", "M_plan"),
        skill("pulse", "M_plan", "State"),
        skill("ops:autopilot", "M_plan", "State"),
        skill("merge", "State", "M^n"),
        skill("gather", "M^n", "M"),
        skill("compact", "State", "M_arch"),
    ]);

    let f = factor_declared(
        "chain:fan",
        &names(&[
            "rail",
            "(pulse | ops:autopilot)",
            "merge",
            "gather",
            "compact",
        ]),
        &by_name,
    );
    // gather yields M and compact wants State: the declared sequence breaks
    // exactly there, and nowhere else. `*` because pulse is on the fallback
    // list; `↺` because compact's M_arch reaches rail's M.
    assert_eq!(f.word, "▸ ⊗ ▸ ▸ ✗ ⊕ * ↺", "{:?}", f.notes);
    assert!(!f.typechecks);

    let f = factor_declared(
        "chain:fan",
        &names(&["rail", "(pulse | ops:autopilot)", "merge", "gather"]),
        &by_name,
    );
    assert_eq!(f.word, "▸ ⊗ ▸ ▸ ⊕ * ↺", "{:?}", f.notes);
    assert!(f.typechecks);
    assert!(f.stages.expand && f.stages.execute && f.stages.select && f.stages.repeat);
    assert_eq!(
        f.shape.as_deref(),
        Some(
            &[
                vec!["rail".to_string()],
                vec!["pulse".to_string(), "ops:autopilot".to_string()],
                vec!["merge".to_string()],
                vec!["gather".to_string()],
            ][..]
        )
    );
    let term = rebuild(&f, &by_name).expect("a declared fan-out rebuilds");
    assert!(
        term.to_string().contains('⊗') || term.label().contains('⊗'),
        "{term}"
    );

    // Declared in sequence, the same two M_plan readers are two stages: pulse
    // yields State and ops:autopilot wants M_plan, so the sequence is broken.
    // The scan would have called this `⊗` and passed it.
    let f = factor_declared(
        "chain:seq",
        &names(&["rail", "pulse", "ops:autopilot"]),
        &by_name,
    );
    assert_eq!(f.word, "▸ ✗ *", "{:?}", f.notes);
    assert!(!f.typechecks);
    let g = factor(
        "chain:seq",
        &names(&["rail", "pulse", "ops:autopilot"]),
        &by_name,
    );
    assert_eq!(g.word, "▸ ⊗ *", "the scan reads the same list as a fan-out");

    // A declared fan-out whose branches read different types is a lie about
    // the shape, and the types say so.
    let f = factor_declared(
        "chain:lie",
        &names(&["rail", "(pulse | compact)"]),
        &by_name,
    );
    assert!(!f.typechecks);
    assert!(
        f.notes.iter().any(|n| n.contains("declared parallel")),
        "{:?}",
        f.notes
    );
}

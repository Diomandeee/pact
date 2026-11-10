//! `pact` — the surface.
//!
//! Four things a person can do with a corpus: see what breaks, see how the
//! chains factor, run one, and get a diagram out.
//!
//! # Last on purpose
//!
//! A CLI over an unproven analysis is a fast way to publish wrong numbers.
//! Everything printed here is computed by a layer with a gate behind it, and
//! every gate below L4 reads "matches the Python" against a live corpus.
//!
//! # No argument-parsing dependency
//!
//! Four subcommands over an optional path. A crate whose job is splitting a
//! `Vec<String>` would be the largest thing in the workspace, and the layer
//! rule points one way for a reason.

use pact_analysis::{dynamics, statics, Complex, Filtration, StuckDetector, TurnMove};
use pact_corpus::{categorise, default_root, factor_all, load, to_graph, Skill};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

mod export;
mod tiers;

const USAGE: &str = "\
pact — read a corpus as an economy

  pact check      [path]   what breaks: dead ends, starved skills, pressure
  pact decompose  [path]   how every chain factors, and what it is missing
  pact simulate   [path]   run the imported corpus and report conservation
  pact export     [path]   a diagram, on stdout
  pact tiers      [path]   what the engine answered alone, and what escalated
  pact stuck              fire lines for stuck threads, read off stdin

`path` defaults to ~/.claude/skills

`pact stuck` reads JSON lines off stdin, one per classified user turn:

  {\"session\": \"<id>\", \"idx\": <n>, \"move\": \"<LABEL>\"}

Sessions may interleave; each session's turns must arrive in idx order.
It prints one line per firing turn — the second of two consecutive
REJECT_CORRECTs, staying lit while the streak holds:

  {\"session\": \"<id>\", \"idx\": <n>}

Malformed lines are skipped and counted on stderr. This is the P9.1
StuckDetector as a callee, so the live watcher classifies text in Python
and decides here, with one rule in one place.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        print!("{USAGE}");
        return ExitCode::from(2);
    };
    // `stuck` reads stdin, not the skills corpus: branching before the
    // load keeps a trigger over conversation turns from depending on a
    // corpus of skills.
    if command.as_str() == "stuck" {
        return stuck();
    }

    // Resolved before the load: a mistyped command is a usage error whether or
    // not a corpus exists on this machine.
    let report: fn(&[Skill]) -> ExitCode = match command.as_str() {
        "check" => check,
        "decompose" => decompose,
        "simulate" => simulate,
        "tiers" => tiers_report,
        "export" => export_graph,
        other => {
            eprintln!("unknown command: {other}\n");
            print!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let root = args
        .get(1)
        .map_or_else(default_root, |p| PathBuf::from(p.clone()));

    let skills = match load(&root) {
        Ok(skills) if !skills.is_empty() => skills,
        Ok(_) => {
            eprintln!("no typed skills under {}", root.display());
            eprintln!("  a file counts when it carries a SOOP-2 block");
            return ExitCode::from(1);
        }
        Err(e) => {
            eprintln!("cannot read {}: {e}", root.display());
            return ExitCode::from(1);
        }
    };

    report(&skills)
}

/// The type graph as Mermaid.
fn export_graph(skills: &[Skill]) -> ExitCode {
    print!("{}", export::mermaid(skills));
    ExitCode::SUCCESS
}

/// What breaks.
///
/// Exit code is the finding: non-zero when something cannot fire, so this is
/// usable in CI without anyone parsing the output.
fn check(skills: &[Skill]) -> ExitCode {
    let category = categorise(skills);
    let stat = statics::analyse(skills, &category);
    let dyn_ = dynamics::analyse(&category, &dynamics::type_edges(skills));

    println!(
        "{} skills, {} types\n",
        stat_len(skills),
        category.objects.len()
    );

    if stat.never_fires.is_empty() {
        println!("  ok    every skill can fire");
    } else {
        println!("  FAIL  {} skill(s) can never fire", stat.never_fires.len());
        for name in &stat.never_fires {
            println!("          {name}");
        }
        println!("        their input type has no producer, so nothing reaches them");
    }

    println!("\n  source-only types (work enters here)");
    for ty in &stat.source_only {
        println!("          {ty}");
    }
    println!("\n  dead ends (work arrives and cannot continue)");
    for ty in &stat.dead_ends {
        println!("          {ty}");
    }

    if dyn_.unbounded.is_empty() {
        println!("\n  ok    no pool under pressure");
    } else {
        println!("\n  pools under pressure");
        for pool in &dyn_.unbounded {
            println!(
                "          {:<14} {:>6}  {} producers into {} consumer(s)",
                pool.id,
                pool.pressure.unwrap_or(0.0),
                pool.inflow,
                pool.outflow
            );
        }
    }

    println!("\n  {}", dyn_.verdict());

    if stat.never_fires.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn stat_len(skills: &[Skill]) -> usize {
    skills.len()
}

/// How every chain factors.
fn decompose(skills: &[Skill]) -> ExitCode {
    let factorings = factor_all(skills);
    let complete = factorings
        .iter()
        .filter(|f| f.stages.present() == 4)
        .count();

    for f in &factorings {
        let types = if f.typechecks { "ok " } else { "BRK" };
        println!(
            "  {} {}  {:<26} {}",
            f.completeness(),
            types,
            f.chain,
            f.word
        );
    }

    println!(
        "\n  {complete} of {} at 4/4; {} typecheck end to end",
        factorings.len(),
        factorings.iter().filter(|f| f.typechecks).count()
    );

    // Which stage is missing most often is the actionable number: it names the
    // generator the corpus is short of, rather than the chains that suffer.
    let mut missing: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &factorings {
        for (label, present) in [
            ("⊗ expand", f.stages.expand),
            ("* execute", f.stages.execute),
            ("⊕ select", f.stages.select),
            ("↺ repeat", f.stages.repeat),
        ] {
            if !present {
                *missing.entry(label).or_default() += 1;
            }
        }
    }
    println!("\n  missing across the corpus");
    for (stage, count) in &missing {
        println!("          {stage:<12} {count}");
    }

    ExitCode::SUCCESS
}

/// Run the imported corpus.
fn simulate(skills: &[Skill]) -> ExitCode {
    use pact_core::{Interpreter, Simulator};

    let graph = to_graph(skills);
    let mut sim = Simulator::new(&graph, 1);

    // Seed every source-only type, because nothing else will: a source-only
    // type is what a person brings, and a run with none of them does nothing
    // and reports that it conserved perfectly.
    let category = categorise(skills);
    for ty in &category.source_only {
        let id = format!("type:{ty}");
        if let Err(e) = sim.create(&id, &pact_core::Bag::of(ty.clone(), 8)) {
            eprintln!("cannot seed {ty}: {e:?}");
            return ExitCode::from(1);
        }
    }

    let opening = sim.total_held();
    let steps = 24;
    let (trace, ledger) = match sim.run(&graph, steps) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("the run failed: {e:?}");
            return ExitCode::from(1);
        }
    };

    let fired = trace
        .events()
        .filter(|e| matches!(e, pact_core::Event::Converted { .. }))
        .count();
    let starved = trace
        .events()
        .filter(|e| matches!(e, pact_core::Event::Starved { .. }))
        .count();

    println!("  {steps} steps, {} events", trace.events().count());
    println!("  {fired} firing(s), {starved} starvation(s)");
    println!("  created   {}", ledger.created().total());
    println!("  destroyed {}", ledger.destroyed().total());

    match ledger.conserved(&opening, &sim.total_held()) {
        Ok(()) => {
            println!("  ok    conserved, per type");
            ExitCode::SUCCESS
        }
        Err(broken) => {
            println!("  FAIL  conservation broken in {} type(s)", broken.len());
            for imbalance in broken.iter().take(8) {
                println!(
                    "          {:<14} expected {} held {}",
                    imbalance.ty, imbalance.expected, imbalance.found
                );
            }
            ExitCode::from(1)
        }
    }
}

/// Kept so the binary exercises the persistence layer it ships.
#[allow(dead_code)]
fn barcode_of(generations: Vec<Complex>) -> Filtration {
    let mut f = Filtration::new();
    for complex in generations {
        let _ = f.admit(complex, &BTreeMap::new());
    }
    f
}

/// What the engine answered alone, and what had to escalate.
fn tiers_report(skills: &[Skill]) -> ExitCode {
    let factorings = factor_all(skills);
    let questions = tiers::interrogate(skills, &factorings);
    let tally = tiers::tally(&questions);

    println!(
        "  {} question(s)
",
        tally.total()
    );
    println!(
        "  engine  {:>5}   {}",
        tally.engine,
        tiers::Tier::Engine.cost()
    );
    println!(
        "  model   {:>5}   {}",
        tally.model,
        tiers::Tier::Model.cost()
    );
    println!(
        "  agent   {:>5}   {}",
        tally.agent,
        tiers::Tier::Agent.cost()
    );
    println!(
        "
  {}% answered from structure alone",
        tally.engine_share()
    );

    // Every escalation, with its reason. An escalation with no reason is
    // indistinguishable from a lazy default.
    let mut reasons: BTreeMap<&str, usize> = BTreeMap::new();
    for q in questions.iter().filter(|q| q.tier != tiers::Tier::Engine) {
        if let Some(reason) = q.reason.as_deref() {
            *reasons.entry(reason).or_default() += 1;
        }
    }
    if !reasons.is_empty() {
        println!(
            "
  why anything left the engine"
        );
        for (reason, count) in &reasons {
            println!("          {count:>4}  {reason}");
        }
    }
    ExitCode::SUCCESS
}

/// The P9.1 trigger as a callee: classified turns in on stdin, fire lines
/// out on stdout. One detector per session id; sessions may interleave.
///
/// A malformed line is skipped and counted on stderr rather than aborting
/// the stream — a watcher piping thousands of turns must not lose the batch
/// to one bad row. Exit code is 0 unless stdin itself cannot be read.
fn stuck() -> ExitCode {
    use std::collections::HashMap;
    use std::io::BufRead as _;

    let mut detectors: HashMap<String, StuckDetector> = HashMap::new();
    let mut skipped = 0u64;
    let mut fired = 0u64;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("stuck: stdin error: {e}");
                return ExitCode::from(1);
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let Some((session, idx, mv)) = parse_turn(&line) else {
            skipped += 1;
            continue;
        };
        let turn = TurnMove::from_label(&mv);
        if detectors.entry(session.clone()).or_default().push(turn) {
            println!("{{\"session\": \"{session}\", \"idx\": {idx}}}");
            fired += 1;
        }
    }
    eprintln!("stuck: {fired} fire(s), {skipped} skipped line(s)");
    ExitCode::SUCCESS
}

/// Parse one flat turn object. Field order is free; anything else shaped is
/// rejected — the caller owns the schema, and silent misreads here would
/// move fire lines between sessions.
fn parse_turn(line: &str) -> Option<(String, u64, String)> {
    let body = line.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut session = None;
    let mut idx = None;
    let mut mv = None;
    for part in body.split(',') {
        let (k, v) = part.split_once(':')?;
        match k.trim().trim_matches('"') {
            "session" => session = Some(v.trim().trim_matches('"').to_string()),
            "idx" => idx = v.trim().parse().ok(),
            "move" => mv = Some(v.trim().trim_matches('"').to_string()),
            _ => {}
        }
    }
    Some((session?, idx?, mv?))
}

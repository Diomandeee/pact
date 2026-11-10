//! Dump skill signatures as JSON for the pPACT server-side validator.
use std::fmt::Write as _;

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
fn main() {
    let skills = pact_corpus::load(&pact_corpus::default_root()).expect("corpus");
    let mut out = String::from("{\"skills\":[");
    for (i, s) in skills.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let outs: Vec<String> = s
            .outputs
            .iter()
            .map(|t| format!("\"{}\"", esc(&t.to_string())))
            .collect();
        write!(
            out,
            "{{\"name\":\"{}\",\"input\":\"{}\",\"outputs\":[{}]}}",
            esc(&s.name),
            esc(&s.input.to_string()),
            outs.join(",")
        )
        .expect("writing to a String cannot fail");
    }
    out.push_str("]}");
    println!("{out}");
}

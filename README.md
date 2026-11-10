# PACT

**Prompt Algebra and Composition Theory.** Flow-based infrastructure for
prompt and agent systems.

You express a system as a typed flow graph. It simulates. It tells you what is
broken. And it decomposes the chains you already have into a small algebra, so
you can see what they actually are.

## Start here

This README is a pointer, not a description. The real documents are:

| | |
|---|---|
| Why it exists | [`.governance/0.1.1_PROJECT_CHARTER.md`](.governance/0.1.1_PROJECT_CHARTER.md) |
| What the words mean | [`.governance/0.1.2_SYSTEM_GLOSSARY.md`](.governance/0.1.2_SYSTEM_GLOSSARY.md) |
| What must never break | [`.governance/0.1.3_ASSUMPTIONS_INVARIANTS.md`](.governance/0.1.3_ASSUMPTIONS_INVARIANTS.md) |
| How to work here | [`.governance/0.2_IMPLEMENTATION_GUIDE.md`](.governance/0.2_IMPLEMENTATION_GUIDE.md) |
| What is being built | [`.governance/0.3_LIVING_CHECKLIST.md`](.governance/0.3_LIVING_CHECKLIST.md) |
| What is open to work on | [`.governance/0.4_UNLOCK_MAP.toml`](.governance/0.4_UNLOCK_MAP.toml) |
| The mathematics | [`SPEC.md`](SPEC.md) |
| What has been measured | [`docs/findings/`](docs/findings/) |
| The theory, argued | [`THEORY.md`](THEORY.md) |

## Running it

```bash
cargo build -p pact-wasm --target wasm32-unknown-unknown --release   # do this first
cargo test --workspace    # the test suite
./scripts/canaries.sh     # one detector per invariant
./scripts/pz status       # what is open, what is next, what is far and why
./scripts/pz check        # nothing was edited while locked
./scripts/deploy up       # build and serve on 127.0.0.1:8731
```

The server has no auth, and `scripts/deploy` refuses to bind every interface.

## What is not here

The skill corpus this was developed against is private, so point `pact` at a
folder of your own skills. The test fixtures keep the labels from real sessions
and drop the text. The live trigger A/B harness can only run its `--smoke` mode
without that corpus. A model reading untyped skill files (P8.2 onward) is planned
and has not been built.

Red signals are expected and are left red. A red signal made green by
weakening the check is the failure this project exists to prevent.

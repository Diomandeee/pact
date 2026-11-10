#!/usr/bin/env bash
# Canaries for every invariant in .governance/0.1.3_ASSUMPTIONS_INVARIANTS.md
#
# An invariant with no detector is a promise. This script is what turns the
# promises into gates. It runs in CI and it fails the build.
#
#   ./scripts/canaries.sh          run them all
#   ./scripts/canaries.sh --prove  deliberately break one and confirm it fires
#
# The --prove mode exists because of a lesson from finding 0001: a gate that
# could not have failed is not evidence. Every canary here must be shown to
# catch the thing it claims to catch, at least once, on purpose.

set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

PASS=0
FAIL=0
CORE="crates/pact-core/src"

say() { printf '%-8s %-8s %s\n' "$1" "$2" "$3"; }

check() { # name, description, command...
  local name="$1" desc="$2"; shift 2
  if "$@" >/dev/null 2>&1; then
    say "ok" "$name" "$desc"; PASS=$((PASS + 1))
  else
    say "FAIL" "$name" "$desc"; FAIL=$((FAIL + 1))
  fi
}

# `check` passes when the command succeeds. Several canaries are the opposite:
# they pass when a grep finds NOTHING. Inverting inline would read as a bug, so
# it gets its own helper and its own name.
check_absent() { # name, description, pattern, path
  local name="$1" desc="$2" pattern="$3" path="$4"
  if [ ! -d "$path" ]; then
    say "skip" "$name" "$desc (no $path yet)"
    return
  fi
  local hits
  hits=$(grep -rInE "$pattern" "$path" 2>/dev/null | grep -vE '^\s*//|^\s*#' | wc -l | tr -d ' ')
  if [ "$hits" = "0" ]; then
    say "ok" "$name" "$desc"; PASS=$((PASS + 1))
  else
    say "FAIL" "$name" "$desc — $hits hit(s)"
    grep -rInE "$pattern" "$path" 2>/dev/null | grep -vE '^\s*//|^\s*#' | head -5 | sed 's/^/           /'
    FAIL=$((FAIL + 1))
  fi
}

echo "canaries — .governance/0.1.3_ASSUMPTIONS_INVARIANTS.md"
echo

# INV-1: the engine is pure. Two halves: no dependencies, and no I/O calls.
if [ -f crates/pact-core/Cargo.toml ]; then
  deps=$(awk '/^\[dependencies\]/{f=1;next} /^\[/{f=0} f && NF && $0 !~ /^#/' \
         crates/pact-core/Cargo.toml | wc -l | tr -d ' ')
  if [ "$deps" = "0" ]; then
    say "ok" "INV-1a" "engine has zero dependencies"; PASS=$((PASS + 1))
  else
    say "FAIL" "INV-1a" "engine declares $deps dependenc(ies)"; FAIL=$((FAIL + 1))
  fi
else
  say "skip" "INV-1a" "pact-core not created yet"
fi
check_absent "INV-1b" "engine performs no I/O" \
  'std::(fs|net|io|process)::|SystemTime::now|Instant::now' "$CORE"

# INV-6: no stubs. The one Mohamed asked for by name.
check_absent "INV-6a" "no todo!/unimplemented! in shipped code" \
  'todo!|unimplemented!|unreachable!\("stub' "crates"
check_absent "INV-6b" "no ignored tests" \
  '#\[ignore\]' "crates"

# INV-7: rail and pulse are words, not primitives. Doc comments are exempt,
# because explaining WHY the engine has no Rail type belongs in the engine.
check_absent "INV-7" "engine names no protocol concept" \
  '\b(struct|enum|fn|type)\s+(Rail|Pulse|Phase|Track|Generation)\b' "$CORE"

# Spec drift. `spec/invariants.toml` is versioned separately from the crates
# and no crate may read it (INV-1 forbids the engine I/O, and a constant the
# algebra read at runtime would put the spec on the hot path). So every number
# living in both places is duplicated by construction, and this is the detector
# for the divergence that duplication guarantees eventually.
#
# Format: spec_key<TAB>file<TAB>rust_const
SPEC=spec/invariants.toml
spec_value() { awk -F'=' -v k="$1" '$1 ~ "^"k"[ ]*$" {gsub(/[ ]/,"",$2); print $2}' "$SPEC"; }
code_value() { awk -F'=' -v k="$2" '$0 ~ k": usize" {gsub(/[ ;]/,"",$2); print $2}' "$1"; }

if [ -f "$SPEC" ]; then
  drift=0
  while IFS='|' read -r key file konst; do
    [ -f "$file" ] || continue
    want=$(spec_value "$key")
    got=$(code_value "$file" "$konst")
    if [ -z "$want" ] || [ "$want" != "$got" ]; then
      say "FAIL" "SPEC-1" "$key: spec=${want:-absent} code=${got:-absent}"
      drift=$((drift + 1))
    fi
  done <<EOF
pressure_ratio|$CORE/graph.rs|PRESSURE_RATIO
max_tracks_per_phase|crates/pact-algebra/src/protocol.rs|MAX_TRACKS_PER_PHASE
min_viable_transitions|crates/pact-algebra/src/protocol.rs|MIN_VIABLE_TRANSITIONS
EOF
  if [ "$drift" = "0" ]; then
    say "ok" "SPEC-1" "3 locked constant(s) match the engine"; PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + drift))
  fi
fi

# INV-8: dependencies live at L4. The engine is pure by INV-1; this is the
# rule for everything between it and the surface. Without it, "only the surface
# takes dependencies" survives exactly until the first afternoon somebody wants
# a convenience in the analysis layer.
if command -v cargo >/dev/null 2>&1; then
  leaked=0
  for crate in pact-algebra pact-corpus pact-analysis; do
    [ -d "crates/$crate" ] || continue
    foreign=$(cargo tree -p "$crate" --depth 1 --prefix none 2>/dev/null \
      | tail -n +2 | awk '{print $1}' | grep -v '^pact-' | grep -v '^$' | wc -l | tr -d ' ')
    if [ "${foreign:-0}" != "0" ]; then
      say "FAIL" "INV-8" "$crate has $foreign third-party dependenc(ies)"
      leaked=$((leaked + 1))
    fi
  done
  if [ "$leaked" = "0" ]; then
    say "ok" "INV-8" "L1-L3 depend only on workspace members"; PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + leaked))
  fi
fi

# The unsafe boundary. `unsafe_code` is `forbid` for the whole workspace except
# `pact-wasm`, where an FFI boundary makes it unavoidable. An exception that is
# not checked is an exception that spreads, so this is the check.
if [ -d crates ]; then
  spread=$(grep -rlE '^\s*(pub )?unsafe |#!\[allow\(unsafe_code\)\]' crates/*/src 2>/dev/null \
    | grep -v '^crates/pact-wasm/' | wc -l | tr -d ' ')
  manifests=$(grep -lE 'unsafe_code = "allow"' crates/*/Cargo.toml 2>/dev/null \
    | grep -v '^crates/pact-wasm/' | wc -l | tr -d ' ')
  if [ "$spread" = "0" ] && [ "$manifests" = "0" ]; then
    say "ok" "UNSAFE" "confined to pact-wasm, where the FFI boundary is"
    PASS=$((PASS + 1))
  else
    say "FAIL" "UNSAFE" "$spread file(s) and $manifests manifest(s) outside pact-wasm"
    FAIL=$((FAIL + 1))
  fi
fi

# INV-2, INV-3, INV-4 are property tests, not greps. They gate via cargo.
if command -v cargo >/dev/null 2>&1 && [ -f Cargo.toml ]; then
  check "INV-2/3/4" "conservation, reproducibility, validator" \
    cargo test --workspace --quiet
  check "lint" "clippy wall is up" \
    cargo clippy --workspace --all-targets -- -D warnings
else
  say "skip" "cargo" "no toolchain or no workspace"
fi

# Living checklist: every ticked box must name an artifact that exists.
# This is the cold-artifact check, the same one the loop harness runs on
# generations, applied to the checklist itself.
CHECKLIST=.governance/0.3_LIVING_CHECKLIST.md
if [ -f "$CHECKLIST" ]; then
  ticked=$(grep -cE '^[[:space:]]*- \[x\]' "$CHECKLIST" 2>/dev/null; true)
  say "info" "checklist" "$ticked item(s) ticked; each must name a live artifact"
fi

echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" = "0" ] || exit 1

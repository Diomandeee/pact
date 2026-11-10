// PACT corpus database.
//
// A SEPARATE DEPLOYMENT from the MotionMix database. Different product, different lifecycle; a PACT schema
// change must not be able to touch that one.
//
// Design rules, four inherited from the MotionMix schema and one added:
//
//  1. Two planes. `corpora` and `skills` are current-state: one doc per
//     entity, mutated in place, small, and the live subscription targets.
//     `snapshots`, `readings`, `evaluations` and `findings` are append-only
//     and carry the volume.
//
//  2. Every history row carries `dedupeKey`. Ingest can re-run over the same
//     corpus forever without duplicating a row.
//
//  3. THE ADDED RULE: a reading is keyed by CONTENT, not by time. The same
//     skill unchanged across ten snapshots has ONE reading. That is what makes
//     the cache work and what makes it affordable; keying by snapshot would
//     re-pay the model for every skill on every import.
//
//  4. Convex is a MIRROR, NEVER A DEPENDENCY. `pact check` works on a plane.
//     The Rust server is the sole writer, and nothing in here computes
//     analysis: Convex functions are TypeScript, and a second engine in
//     TypeScript is precisely the failure this project exists to prevent.
//
//  5. Cross-plane handles are object addresses, "pact/skill/<contentHash>",
//     never shared tables.
//
// Not defined, deliberately: `generations` and `runs`. The barcode needs a
// live subject run more than once, which is not this phase, and defining
// tables for work nobody is doing is the speculative structure this project
// has refused everywhere else.
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  // ────────────────────────────────────────────────────────── current state

  corpora: defineTable({
    slug: v.string(), // "mohamed" — stable, url-safe
    label: v.string(),
    root: v.optional(v.string()), // absent for an uploaded corpus
    skillCount: v.number(),
    lastSnapshotAt: v.optional(v.number()),
    updatedAt: v.number(),
  }).index("by_slug", ["slug"]),

  // One doc per (corpus, skill), mutated in place. What a dashboard
  // subscribes to.
  //
  // Carries BOTH what the author declared and what the model read, and never
  // merges them. The difference between the two IS the finding, and a schema
  // that collapsed them into one field would destroy the only output of P8
  // anyone would look at.
  skills: defineTable({
    corpusSlug: v.string(),
    name: v.string(),
    contentHash: v.string(), // sha256 over the whole file

    // Absent for an untyped corpus, which is the case that matters: a folder
    // of ordinary prompts has no frontmatter at all.
    declared: v.optional(
      v.object({
        inputType: v.string(),
        outputTypes: v.array(v.string()),
        category: v.optional(v.string()),
        idempotent: v.optional(v.boolean()),
      }),
    ),

    inferred: v.optional(
      v.object({
        inputType: v.string(),
        outputTypes: v.array(v.string()),
        category: v.string(),
        confidence: v.number(), // 0..1, the model's own
        summary: v.string(), // one line, for the inspector
        readingId: v.string(), // which reading produced it
      }),
    ),

    // Set only when both exist and disagree. The column somebody actually
    // reads, and the reason `by_corpus_disagrees` is an index rather than a
    // filter: "show me where the model and I differ" is the whole product.
    disagrees: v.optional(v.boolean()),

    objectAddress: v.optional(v.string()),
    updatedAt: v.number(),
  })
    .index("by_corpus_name", ["corpusSlug", "name"])
    .index("by_contentHash", ["contentHash"])
    .index("by_corpus_disagrees", ["corpusSlug", "disagrees"]),

  // ────────────────────────────────────────────────────────── history

  snapshots: defineTable({
    dedupeKey: v.string(), // corpusSlug + corpusHash
    corpusSlug: v.string(),
    corpusHash: v.string(), // hash over every skill's contentHash
    at: v.number(),
    skillCount: v.number(),
    typedCount: v.number(), // how many carried frontmatter
    note: v.optional(v.string()),
  })
    .index("by_dedupeKey", ["dedupeKey"])
    .index("by_corpus_at", ["corpusSlug", "at"]),

  // Content-keyed. This is rule 3, and it is the difference between paying
  // once per distinct skill body and paying once per skill per import.
  readings: defineTable({
    dedupeKey: v.string(), // contentHash + model + promptVersion
    contentHash: v.string(),
    model: v.string(), // "gpt-4.1-mini"
    promptVersion: v.string(), // "v3"

    inputType: v.string(),
    outputTypes: v.array(v.string()),
    category: v.string(),
    confidence: v.number(),
    summary: v.string(),
    similarTo: v.optional(v.array(v.string())), // duplicate candidates

    // What it cost, so the bulk/escalate split can be judged on evidence
    // rather than defended.
    promptTokens: v.optional(v.number()),
    completionTokens: v.optional(v.number()),
    at: v.number(),
  })
    .index("by_dedupeKey", ["dedupeKey"])
    .index("by_content_model", ["contentHash", "model"])
    .index("by_promptVersion", ["promptVersion"]),

  // Accuracy per prompt version. The table that makes prompt work measurable
  // rather than a matter of opinion.
  //
  // Each accuracy is stored next to the floor it has to beat. A number read
  // without its baseline is how 60% on `input_type` gets mistaken for a
  // result when the trivial guess already scores 55.3%.
  evaluations: defineTable({
    dedupeKey: v.string(), // snapshotHash + model + promptVersion
    corpusSlug: v.string(),
    snapshotHash: v.string(),
    model: v.string(),
    promptVersion: v.string(),

    graded: v.number(), // how many skills had a declared type

    inputAccuracy: v.number(),
    outputAccuracy: v.number(),
    categoryAccuracy: v.number(),

    inputBaseline: v.number(),
    outputBaseline: v.number(),
    categoryBaseline: v.number(),

    at: v.number(),
  })
    .index("by_dedupeKey", ["dedupeKey"])
    .index("by_corpus_at", ["corpusSlug", "at"])
    .index("by_promptVersion", ["promptVersion"]),

  // What the analysis said at each snapshot. "How has my corpus changed since
  // March" is a range query over this, and nothing today can answer it.
  findings: defineTable({
    dedupeKey: v.string(), // snapshotHash + kind
    corpusSlug: v.string(),
    snapshotHash: v.string(),
    at: v.number(),
    kind: v.string(), // "pressure" | "dead" | "chains" | ...

    deadConverters: v.optional(v.array(v.string())),
    sourceOnly: v.optional(v.array(v.string())),
    deadEnds: v.optional(v.array(v.string())),
    pressure: v.optional(
      v.array(
        v.object({
          type: v.string(),
          ratio: v.number(),
          producers: v.number(),
          consumers: v.number(),
        }),
      ),
    ),
    feedbackLoops: v.optional(v.number()),
    chainsAt44: v.optional(v.number()),
    chainsTotal: v.optional(v.number()),
  })
    .index("by_dedupeKey", ["dedupeKey"])
    .index("by_corpus_kind_at", ["corpusSlug", "kind", "at"]),
});

// Readings, keyed by content.
//
// This file is the enforcement half of rule 3 in `schema.ts`. The index named
// `by_dedupeKey` declares an intent and enforces nothing: Convex has no unique
// constraint, so "one row per distinct reading" is true only if every writer
// looks before it writes. One writer that inserts blindly and the cache is a
// log, and a log re-pays the model for every skill on every import.
//
// So the only way in is `record`, and it reads first.
//
// # Why this is not analysis
//
// Rule 4 says nothing in Convex computes analysis, because a second engine in
// TypeScript is the failure this project exists to prevent. Idempotency is not
// analysis: this file decides whether a row already exists, never what a row
// means. The types it stores were inferred elsewhere and it does not check
// them, compose them, or reason about them.
import { mutation, query } from "./_generated/server";
import { v } from "convex/values";

/// The identity of a reading.
///
/// Content, model, prompt version. NOT time, NOT snapshot, NOT corpus: the
/// same skill body read by the same model under the same prompt is the same
/// reading whether it was found once or in every import since March.
///
/// Kept as one function rather than inlined at each call site so the identity
/// cannot drift between the writer and the reader, which is the way a
/// content-keyed store quietly becomes a time-keyed one.
export function dedupeKeyOf(
  contentHash: string,
  model: string,
  promptVersion: string,
): string {
  return `${contentHash}|${model}|${promptVersion}`;
}

/// Store a reading, or return the one already stored.
///
/// Returns `{ id, created }`. `created` is false on a hit, which is what makes
/// the cache measurable: the caller can count calls it did not have to pay
/// for instead of asserting that it saved them.
export const record = mutation({
  args: {
    contentHash: v.string(),
    model: v.string(),
    promptVersion: v.string(),
    inputType: v.string(),
    outputTypes: v.array(v.string()),
    category: v.string(),
    confidence: v.number(),
    summary: v.string(),
    similarTo: v.optional(v.array(v.string())),
    promptTokens: v.optional(v.number()),
    completionTokens: v.optional(v.number()),
    at: v.number(),
  },
  handler: async (ctx, args) => {
    const dedupeKey = dedupeKeyOf(
      args.contentHash,
      args.model,
      args.promptVersion,
    );

    const existing = await ctx.db
      .query("readings")
      .withIndex("by_dedupeKey", (q) => q.eq("dedupeKey", dedupeKey))
      .unique();

    // A hit returns the row untouched. Not "update it with the new one":
    // rewriting a row whose key says it is the same reading would make `at`
    // mean last-seen instead of first-read, and then the table times what it
    // is supposed to key by content.
    if (existing !== null) {
      return { id: existing._id, created: false };
    }

    const id = await ctx.db.insert("readings", { dedupeKey, ...args });
    return { id, created: true };
  },
});

/// How many readings exist for this content under this model.
///
/// The gate counts with it. Indexed rather than filtered so it stays a lookup
/// as the table grows.
export const countFor = query({
  args: { contentHash: v.string(), model: v.string() },
  handler: async (ctx, args) => {
    const rows = await ctx.db
      .query("readings")
      .withIndex("by_content_model", (q) =>
        q.eq("contentHash", args.contentHash).eq("model", args.model),
      )
      .collect();
    return rows.length;
  },
});

/// Remove every reading for one content hash.
///
/// Exists so the gate can run twice and mean the same thing both times. A
/// gate that only passes on an empty database is a gate that passes once.
export const forget = mutation({
  args: { contentHash: v.string() },
  handler: async (ctx, args) => {
    // A prefix of `by_content_model`. `readings` has no contentHash-only
    // index and does not need one: an index on (contentHash, model) answers a
    // contentHash query already, and a second index would be a duplicate
    // Convex maintains on every write.
    const rows = await ctx.db
      .query("readings")
      .withIndex("by_content_model", (q) => q.eq("contentHash", args.contentHash))
      .collect();
    for (const row of rows) {
      await ctx.db.delete(row._id);
    }
    return rows.length;
  },
});

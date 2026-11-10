#!/usr/bin/env python3
"""P9.3 trigger A/B harness: two-reject (T) vs ranker (R), under
tests/fixtures/live_gate_ab.md. No tuning on the items; the win condition
and tiebreak are decided in the spec, not here.

Modes:
  --smoke    canned arms on synthetic items; proves the scoring math and
             that the winner declaration flips in both directions.
  --embed    one API batch: embed the 224 item turns, cache to
             ranker-embed-cache.json. The only mode that touches network.
  --offline  the scored round. Uses the cache; no API.

Item lists intentionally read the prototype paths (/tmp, ~/Desktop):
re-running against moved data must fail loudly, not silently score a
different corpus. Vendoring 17k sessions was never on the table.
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
A1 = os.environ.get("PACT_AB_LABELS", "")
CTRL = "/tmp/control_items_113.json"
BASE = os.path.expanduser("~/.claude/prompt-logs/sessions")
CACHE = os.path.join(HERE, "ranker-embed-cache.json")
RANKER = os.environ.get("PACT_AB_RANKER", "")
TAU = 0.5
MARGIN_PP = 5.0

sys.path.insert(0, "/tmp/trajectory-prototype")


def load_items():
    lab = [json.loads(l) for l in open(A1)]
    block = [(r["session_id"], r["turn"]) for r in lab
             if r["verdict"] == "divergence" and r.get("maneuver_type")]
    ctrl = [(r["session_id"], r["turn"]) for r in
            json.load(open(CTRL))]
    return block, ctrl


def turn_text(sid, t):
    rows = [json.loads(l) for l in open(f"{BASE}/{sid}/prompts.jsonl")]
    if 0 <= t < len(rows):
        return rows[t].get("prompt_text", "") or ""
    return ""


def arm_T(sid, t):
    from track import classify
    cur, _, _, _ = classify(turn_text(sid, t)[:2000])
    prev, _, _, _ = classify(turn_text(sid, t - 1)[:2000]) if t > 0 else ("", 0, 0, 0)
    return cur == "REJECT_CORRECT" and prev == "REJECT_CORRECT"


def arm_R(sid, t, cache, model):
    import numpy as np
    key = f"{sid}:{t}"
    if key not in cache:
        raise KeyError(f"no cached embedding for {key}; run --embed first")
    v = np.array(cache[key], dtype=float)
    v = v / max(1e-12, float((v ** 2).sum() ** 0.5))
    return bool(model.predict_proba([v])[0][1] >= TAU)


def score(fires_block, fires_ctrl, n_block, n_ctrl):
    fb = sum(fires_block) / max(1, n_block)
    sc = 1 - sum(fires_ctrl) / max(1, n_ctrl)
    return {"fire_on_block": fb, "silence_on_ctrl": sc,
            "balanced_acc": (fb + sc) / 2}


def declare(sT, sR):
    d = (sT["balanced_acc"] - sR["balanced_acc"]) * 100
    if d >= MARGIN_PP:
        return "T", f"T leads by {d:.1f}pp (≥ {MARGIN_PP})"
    if d <= -MARGIN_PP:
        return "R", f"R leads by {-d:.1f}pp (≥ {MARGIN_PP})"
    return "T", f"tie (gap {d:+.1f}pp < {MARGIN_PP}); tiebreak to T per spec"


def smoke():
    # Canned arms with known scores; winner must flip both directions.
    items_b = [("b%d" % i) for i in range(100)]
    items_c = [("c%d" % i) for i in range(100)]
    # value = FIRES. strong: 80/100 block, 10/100 ctrl -> bal 0.85.
    # weak: 40/100 block, 50/100 ctrl -> bal 0.45.
    strong = {k: (i < 80) for i, k in enumerate(items_b)}
    strong.update({k: (i < 10) for i, k in enumerate(items_c)})
    weak = {k: (i < 40) for i, k in enumerate(items_b)}
    weak.update({k: (i < 50) for i, k in enumerate(items_c)})
    s = score([strong[k] for k in items_b], [strong[k] for k in items_c],
              100, 100)
    w = score([weak[k] for k in items_b], [weak[k] for k in items_c],
              100, 100)
    assert abs(s["balanced_acc"] - 0.85) < 1e-9, s
    assert abs(w["balanced_acc"] - 0.45) < 1e-9, w
    win1, _ = declare(s, w)
    win2, _ = declare(w, s)
    assert win1 == "T" and win2 == "R", (win1, win2)
    # Tiebreak: 2pp gap must go to T regardless of order.
    mid = {"balanced_acc": s["balanced_acc"] - 0.02}
    assert declare(s, mid)[0] == "T"
    assert declare(mid, s)[0] == "T"
    print("smoke ok: math 0.85/0.45, winner flips T/R, tiebreak holds")


def embed():
    import numpy as np  # noqa: F401 (kept local: only this mode needs it)
    from openai import OpenAI
    if not os.environ.get("OPENAI_API_KEY"):
        raise SystemExit("refusing: OPENAI_API_KEY not set")
    block, ctrl = load_items()
    items = [("block", s, t) for s, t in block] + \
            [("ctrl", s, t) for s, t in ctrl]
    print(f"items: {len(block)} block + {len(ctrl)} ctrl", flush=True)
    cache = json.load(open(CACHE)) if os.path.exists(CACHE) else {}
    missing = [(s, t) for _, s, t in items if f"{s}:{t}" not in cache]
    print(f"cached: {len(cache)}, to embed: {len(missing)}", flush=True)
    client = OpenAI()
    for i in range(0, len(missing), 64):
        batch = missing[i:i + 64]
        texts = [turn_text(s, t)[:4000] for s, t in batch]
        resp = client.embeddings.create(model="text-embedding-3-small",
                                        input=texts)
        for (s, t), e in zip(batch, sorted(resp.data, key=lambda d: d.index)):
            cache[f"{s}:{t}"] = e.embedding
        print(f"  {min(i + 64, len(missing))}/{len(missing)}", flush=True)
    json.dump(cache, open(CACHE, "w"))
    print(f"cache: {len(cache)} vectors ({len(missing)} sent this run)")


def offline():
    import pickle
    block, ctrl = load_items()
    cache = json.load(open(CACHE))
    model = pickle.load(open(RANKER, "rb"))
    res = {"tau": TAU, "margin_pp": MARGIN_PP, "arms": {}}
    for name, fn in [("T", lambda s, t: arm_T(s, t)),
                     ("R", lambda s, t: arm_R(s, t, cache, model))]:
        fb, fc = [], []
        for s, t in block:
            try:
                fb.append(fn(s, t))
            except Exception as e:
                print(f"WARN {name} block {s[:8]}:{t}: {e}")
        for s, t in ctrl:
            try:
                fc.append(fn(s, t))
            except Exception as e:
                print(f"WARN {name} ctrl {s[:8]}:{t}: {e}")
        res["arms"][name] = score(fb, fc, len(block), len(ctrl))
        res["arms"][name]["n_block"] = len(fb)
        res["arms"][name]["n_ctrl"] = len(fc)
        print(f"{name}: fire_on_block={res['arms'][name]['fire_on_block']:.3f} "
              f"silence_on_ctrl={res['arms'][name]['silence_on_ctrl']:.3f} "
              f"balanced={res['arms'][name]['balanced_acc']:.3f}")
    winner, why = declare(res["arms"]["T"], res["arms"]["R"])
    res["winner"] = winner
    res["reason"] = why
    print(f"WINNER: {winner} — {why}")
    json.dump(res, open(os.path.join(HERE, "live_gate_ab_results.json"), "w"),
              indent=1)


if __name__ == "__main__":
    mode = sys.argv[1] if len(sys.argv) > 1 else "--smoke"
    if mode == "--embed":
        # guard: never run the API batch from inside cargo test
        if os.environ.get("CARGO_PKG_NAME") or "cargo" in sys.argv[0]:
            raise SystemExit("refusing: --embed under cargo; run by hand")
        embed()
    elif mode == "--offline":
        offline()
    else:
        smoke()

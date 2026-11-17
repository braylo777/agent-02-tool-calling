"""Live eval harness: sends every case to the deployed demo, scores it, writes evals/results.json.
Usage: python3 evals/run.py [baseUrl]. Paced at 1 request / 13 s (demo limit is 5 req/min)."""
import json, math, sys, time, urllib.request
from datetime import datetime, timezone
from pathlib import Path

here = Path(__file__).parent
cfg = json.loads((here / "cases.json").read_text())
base = sys.argv[1] if len(sys.argv) > 1 else cfg["url"]
norm = lambda s: str(s).lower().replace(",", "")
def pct(a, p):
    s = sorted(a)
    return s[min(len(s) - 1, math.ceil(p / 100 * len(s)) - 1)]
rows = []
for i, c in enumerate(cfg["cases"]):
    if i:
        time.sleep(13)
    t0 = time.time(); j = {}; status = 0
    try:
        req = urllib.request.Request(base + cfg["endpoint"], json.dumps({cfg["field"]: c["input"]}).encode(), {"content-type": "application/json"})
        try:
            r = urllib.request.urlopen(req, timeout=60); status = r.status; j = json.load(r)
        except urllib.error.HTTPError as e:
            status = e.code; j = json.load(e)
    except Exception as e:
        j = {"error": str(e)}
    ms = round((time.time() - t0) * 1000); ans = j.get("answer") or ""
    tools = [s["tool"] for s in j.get("trace", []) if s.get("tool")]
    answer_ok = status == 200 and all(any(norm(e) in norm(ans) for e in (g if isinstance(g, list) else [g])) for g in c.get("expect_any", []))
    tool_ok = None
    if "tools" in c:
        tool_ok = ",".join(sorted(set(tools))) == ",".join(sorted(c["tools"]))
    passed = answer_ok and tool_ok is not False
    rows.append({"id": c["id"], "category": c["category"], "input": c["input"], "status": status, "ms": ms, "tools_used": tools,
                 "expected_tools": c.get("tools"), "answer_ok": answer_ok, "tool_ok": tool_ok, "pass": passed, "answer": ans[:400], "error": j.get("error")})
    print(c["id"], "PASS" if passed else "FAIL", f"{ms}ms", tools, ans[:80].replace("\n", " "))
ok = [r for r in rows if r["status"] == 200]; lat = [r["ms"] for r in ok]; tr = [r for r in rows if r["tool_ok"] is not None]
m = {"n": len(rows), "http_200": len(ok), "pass_rate": sum(r["pass"] for r in rows) / len(rows),
     "answer_accuracy": sum(r["answer_ok"] for r in rows) / len(rows),
     "tool_selection_accuracy": (sum(r["tool_ok"] for r in tr) / len(tr)) if tr else None, "tool_cases": len(tr),
     "latency_ms_p50": pct(lat, 50) if lat else None, "latency_ms_p95": pct(lat, 95) if lat else None}
out = {"project": cfg["project"], "model": "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "target": base,
       "run_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"),
       "latency_note": "end-to-end HTTP from the eval machine to the live Worker", "metrics": m, "cases": rows}
(here / "results.json").write_text(json.dumps(out, indent=2))
print(json.dumps(m, indent=2))

// Live eval harness: sends every case to the deployed demo, scores it, writes evals/results.json.
// Usage: node evals/run.mjs [baseUrl]. Paced at 1 request / 13 s to respect the 5 req/min demo limit.
import fs from "node:fs";
const cfg = JSON.parse(fs.readFileSync(new URL("./cases.json", import.meta.url)));
const base = process.argv[2] || cfg.url;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pct = (a, p) => { const s = [...a].sort((x, y) => x - y); return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)]; };
const norm = (s) => String(s).toLowerCase().replace(/,/g, "");
const rows = [];
for (const [i, c] of cfg.cases.entries()) {
  if (i) await sleep(13000);
  const t0 = Date.now(); let j = {}, status = 0;
  try { const r = await fetch(base + cfg.endpoint, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ [cfg.field]: c.input }) }); status = r.status; j = await r.json(); } catch (e) { j = { error: String(e) }; }
  const ms = Date.now() - t0, ans = j.answer || "";
  const tools = (j.trace || []).filter((s) => s.tool).map((s) => s.tool);
  const answerOk = status === 200 && (c.expect_any || []).every((group) => [].concat(group).some((e) => norm(ans).includes(norm(e))));
  let toolOk = null;
  if ("tools" in c) toolOk = [...new Set(tools)].sort().join(",") === [...c.tools].sort().join(",");
  const extra = {};
  if (cfg.kind === "crew") {
    extra.plan_len = (j.plan || []).length;
    extra.plan_ok = extra.plan_len >= 2 && extra.plan_len <= 3;
    const h = (j.handoffs || []).map((x) => x.from);
    extra.handoff_ok = h[0] === "user" && h[1] === "planner" && h.at(-1) === "writer" && h.filter((x) => x.startsWith("researcher")).length === extra.plan_len;
    extra.words = ans.split(/\s+/).filter(Boolean).length; extra.length_ok = extra.words > 0 && extra.words <= 230;
  }
  const pass = answerOk && toolOk !== false && (cfg.kind !== "crew" || (extra.plan_ok && extra.handoff_ok && extra.length_ok));
  rows.push({ id: c.id, category: c.category, input: c.input, status, ms, tools_used: tools, expected_tools: c.tools, answer_ok: answerOk, tool_ok: toolOk, ...extra, pass, answer: ans.slice(0, 400), error: j.error });
  console.log(`${c.id} ${pass ? "PASS" : "FAIL"} ${ms}ms tools=[${tools}] ${ans.slice(0, 80).replace(/\n/g, " ")}`);
}
const ok = rows.filter((r) => r.status === 200), lat = ok.map((r) => r.ms), tr = rows.filter((r) => r.tool_ok !== null);
const m = { n: rows.length, http_200: ok.length, pass_rate: rows.filter((r) => r.pass).length / rows.length,
  answer_accuracy: rows.filter((r) => r.answer_ok).length / rows.length,
  tool_selection_accuracy: tr.length ? tr.filter((r) => r.tool_ok).length / tr.length : null, tool_cases: tr.length,
  latency_ms_p50: lat.length ? pct(lat, 50) : null, latency_ms_p95: lat.length ? pct(lat, 95) : null };
if (cfg.kind === "crew") Object.assign(m, { plan_ok_rate: rows.filter((r) => r.plan_ok).length / rows.length, handoff_ok_rate: rows.filter((r) => r.handoff_ok).length / rows.length, length_ok_rate: rows.filter((r) => r.length_ok).length / rows.length });
const out = { project: cfg.project, model: "@cf/meta/llama-3.3-70b-instruct-fp8-fast", target: base, run_at: new Date().toISOString(), latency_note: "end-to-end HTTP from the eval machine to the live Worker", metrics: m, cases: rows };
fs.writeFileSync(new URL("./results.json", import.meta.url), JSON.stringify(out, null, 2));
console.log(JSON.stringify(m, null, 2));

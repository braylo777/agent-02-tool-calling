import { test } from "node:test";
import assert from "node:assert/strict";
import { runToolAgent, makeTools, toolCalls, TOOL_DEFS } from "../src/agent.js";
import { calc } from "../src/calc.js";

test("tool defs are valid JSON-schema objects", () => {
  assert.deepEqual(TOOL_DEFS.map((t) => t.name), ["calculator", "get_weather", "wikipedia"]);
  for (const t of TOOL_DEFS) { assert.equal(t.parameters.type, "object"); assert.ok(t.parameters.required.length); }
});
test("calc edge cases", () => { assert.equal(makeTools().calculator({ expression: "0.18*240" }), "43.2"); assert.throws(() => calc("2**")); });
test("toolCalls: string arguments are JSON-parsed", () => {
  assert.deepEqual(toolCalls({ tool_calls: [{ name: "wikipedia", arguments: '{"topic":"Rust"}' }] }), [{ name: "wikipedia", arguments: { topic: "Rust" } }]);
});
test("toolCalls: bad arguments become {} and nameless calls are dropped", () => {
  assert.deepEqual(toolCalls({ tool_calls: [{ name: "calculator", arguments: "{oops" }, { arguments: {} }] }), [{ name: "calculator", arguments: {} }]);
});
test("toolCalls: single object in response text", () => {
  assert.deepEqual(toolCalls({ response: 'Sure {"name":"get_weather","arguments":{"city":"Oslo"}}' }), [{ name: "get_weather", arguments: { city: "Oslo" } }]);
});
test("weather: unknown city", async () => {
  const f = async () => ({ ok: true, json: async () => ({ results: [] }) });
  assert.equal(await makeTools(f).get_weather({ city: "Nowhere" }), "No city named Nowhere");
});
test("wikipedia: summary truncated to 600 chars, 404 handled", async () => {
  const ok = async () => ({ ok: true, json: async () => ({ extract: "x".repeat(900) }) });
  assert.equal((await makeTools(ok).wikipedia({ topic: "A B" })).length, 600);
  const nf = async () => ({ ok: false });
  assert.equal(await makeTools(nf).wikipedia({ topic: "Zzz" }), "No article for Zzz");
});
test("wikipedia: spaces become underscores in URL", async () => {
  let url; const f = async (u) => { url = u; return { ok: true, json: async () => ({ extract: "e" }) }; };
  await makeTools(f).wikipedia({ topic: "Ada Lovelace" }); assert.match(url, /Ada_Lovelace$/);
});
test("parallel tool calls in one round + unknown tool", async () => {
  const replies = [{ tool_calls: [{ name: "calculator", arguments: { expression: "2+2" } }, { name: "nope", arguments: {} }] }, { response: "done" }];
  const env = { AI: { run: async () => replies.shift() } };
  const r = await runToolAgent(env, "q", makeTools(async () => ({})));
  assert.deepEqual(r.trace.map((t) => t.result), ["4", "unknown tool nope"]); assert.equal(r.answer, "done");
});
test("step limit when model keeps calling tools", async () => {
  let k = 0; const env = { AI: { run: async () => ({ tool_calls: [{ name: "calculator", arguments: { expression: `${k++}+1` } }] }) } };
  const r = await runToolAgent(env, "q", makeTools(), 2);
  assert.equal(r.answer, "Step limit reached."); assert.equal(r.trace.length, 2);
});
test("loop guard: repeated identical call forces a tool-free final answer (eval regression)", async () => {
  const seenOpts = []; const call = { tool_calls: [{ name: "calculator", arguments: { expression: "1+1" } }] };
  const replies = [call, call, { response: "2" }];
  const env = { AI: { run: async (_m, inp) => { seenOpts.push(!!inp.tools); return replies.shift(); } } };
  const r = await runToolAgent(env, "q");
  assert.equal(r.answer, "2"); assert.equal(r.trace.length, 1); assert.deepEqual(seenOpts, [true, true, false]);
});

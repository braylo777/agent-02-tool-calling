import { test } from "node:test";
import assert from "node:assert/strict";
import { runToolAgent, makeTools, toolCalls } from "../src/agent.js";
import { calc } from "../src/calc.js";

test("calc", () => { assert.equal(calc("2+3*(4-1)"), 11); assert.throws(() => calc("alert(1)")); });
test("tool loop runs native tool calls then answers", async () => {
  const replies = [{ tool_calls: [{ name: "calculator", arguments: { expression: "6*7" } }] }, { response: "42" }];
  const env = { AI: { run: async (_m, inp) => { if (replies.length === 1) assert.match(inp.messages.at(-1).content, /calculator: 42/); return replies.shift(); } } };
  const r = await runToolAgent(env, "6*7?");
  assert.equal(r.answer, "42"); assert.equal(r.trace[0].result, "42");
});
test("parses tool calls emitted as text", () => {
  const r = { response: ['{"type":"function","name":"calculator","parameters":{"expression":"2+2"}}'] };
  assert.deepEqual(toolCalls(r), [{ name: "calculator", arguments: { expression: "2+2" } }]);
  assert.deepEqual(toolCalls({ response: "Hello there" }), []);
});
test("weather tool with mocked HTTP", async () => {
  const f = async (u) => ({ ok: true, json: async () => u.includes("geocoding") ? { results: [{ name: "Austin", country: "US", latitude: 1, longitude: 2 }] } : { current: { temperature_2m: 20, wind_speed_10m: 5 } } });
  assert.match(await makeTools(f).get_weather({ city: "Austin" }), /Austin, US: 20°C/);
});

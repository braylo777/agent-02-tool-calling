import { chat, text } from "./core.js";
import { calc } from "./calc.js";

// JSON-schema tool definitions sent to the model (native function calling).
export const TOOL_DEFS = [
  { name: "calculator", description: "Evaluate an arithmetic expression", parameters: { type: "object", properties: { expression: { type: "string" } }, required: ["expression"] } },
  { name: "get_weather", description: "Current weather for a city", parameters: { type: "object", properties: { city: { type: "string" } }, required: ["city"] } },
  { name: "wikipedia", description: "Short Wikipedia summary of a topic", parameters: { type: "object", properties: { topic: { type: "string" } }, required: ["topic"] } },
];

export function makeTools(fetcher = fetch) {
  return {
    calculator: ({ expression }) => String(+calc(expression).toPrecision(12)), // trim float noise (0.18*240 -> 43.2)
    get_weather: async ({ city }) => {
      const g = await (await fetcher(`https://geocoding-api.open-meteo.com/v1/search?count=1&name=${encodeURIComponent(city)}`)).json();
      const p = g.results?.[0]; if (!p) return `No city named ${city}`;
      const w = await (await fetcher(`https://api.open-meteo.com/v1/forecast?latitude=${p.latitude}&longitude=${p.longitude}&current=temperature_2m,wind_speed_10m`)).json();
      return `${p.name}, ${p.country}: ${w.current.temperature_2m}°C, wind ${w.current.wind_speed_10m} km/h`;
    },
    wikipedia: async ({ topic }) => {
      const r = await fetcher(`https://en.wikipedia.org/api/rest_v1/page/summary/${encodeURIComponent(topic.replace(/ /g, "_"))}`, { headers: { "user-agent": "agent-02-tool-calling (github.com/braylo777)" } });
      return r.ok ? (await r.json()).extract?.slice(0, 600) || "No summary" : `No article for ${topic}`;
    },
  };
}

// Normalize tool calls: native `tool_calls`, or JSON emitted in the text by some models.
export function toolCalls(r) {
  let raw = r?.tool_calls;
  if (!raw?.length && r?.response != null) {
    let v = r.response;
    if (typeof v === "string") { const m = v.match(/[\[{][\s\S]*[\]}]/); try { v = m ? JSON.parse(m[0]) : null; } catch { v = null; } }
    if (v && !Array.isArray(v)) v = [v];
    raw = (v || []).map((x) => { if (typeof x !== "string") return x; try { return JSON.parse(x); } catch { return null; } });
  }
  return (raw || []).filter((c) => c && c.name).map((c) => {
    let a = c.arguments ?? c.parameters ?? {};
    if (typeof a === "string") { try { a = JSON.parse(a || "{}"); } catch { a = {}; } }
    return { name: c.name, arguments: a };
  });
}

// Loop: model picks tools -> we run them -> results go back -> model answers.
export async function runToolAgent(env, question, tools = makeTools(), maxSteps = 4) {
  const messages = [{ role: "system", content: "You are a helpful assistant. Use a tool only when it helps; for general knowledge, greetings or translation, answer directly without tools. Answer concisely." }, { role: "user", content: question }];
  const trace = [], seen = new Set();
  for (let i = 0; i < maxSteps; i++) {
    const r = await chat(env, messages, { tools: TOOL_DEFS });
    const calls = toolCalls(r);
    if (!calls.length) return { answer: text(r), trace };
    // Loop guard: if the model only repeats calls it already made, force a final answer without tools.
    const key = (c) => c.name + JSON.stringify(c.arguments);
    if (calls.every((c) => seen.has(key(c)))) {
      messages.push({ role: "user", content: "You already have all tool results above. Answer the original question now." });
      return { answer: text(await chat(env, messages)), trace };
    }
    calls.forEach((c) => seen.add(key(c)));
    messages.push({ role: "assistant", content: JSON.stringify(calls) });
    for (const c of calls) {
      const args = c.arguments;
      let out; try { out = tools[c.name] ? await tools[c.name](args) : `unknown tool ${c.name}`; } catch (e) { out = `error: ${e.message}`; }
      trace.push({ tool: c.name, args, result: out });
    }
    messages.push({ role: "user", content: "Tool results:\n" + trace.slice(-calls.length).map((t) => `${t.tool}: ${t.result}`).join("\n") + "\nNow answer the original question. Do not call tools again unless needed." });
  }
  return { answer: "Step limit reached.", trace };
}

"""Tool-calling agent: model picks tools (JSON schema) -> we run them -> results go back -> model answers."""
import json, re
from urllib.parse import quote

from .core import MODEL, calc, fmt, js_json, text, CalcError

TOOL_DEFS = [
    {"name": "calculator", "description": "Evaluate an arithmetic expression", "parameters": {"type": "object", "properties": {"expression": {"type": "string"}}, "required": ["expression"]}},
    {"name": "get_weather", "description": "Current weather for a city", "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}},
    {"name": "wikipedia", "description": "Short Wikipedia summary of a topic", "parameters": {"type": "object", "properties": {"topic": {"type": "string"}}, "required": ["topic"]}},
]
SYSTEM = "You are a helpful assistant. Use a tool only when it helps; for general knowledge, greetings or translation, answer directly without tools. Answer concisely."
_enc = lambda s: quote(s, safe="-_.!~*'()")


def make_tools(fetcher):
    """fetcher(url, headers=None) -> (ok: bool, json: dict)."""
    def get_weather(args):
        city = args.get("city", "")
        _, g = fetcher(f"https://geocoding-api.open-meteo.com/v1/search?count=1&name={_enc(city)}")
        res = (g or {}).get("results") or []
        if not res:
            return f"No city named {city}"
        p = res[0]
        _, w = fetcher(f"https://api.open-meteo.com/v1/forecast?latitude={p['latitude']}&longitude={p['longitude']}&current=temperature_2m,wind_speed_10m")
        c = w["current"]
        num = lambda x: fmt(float(x))
        return f"{p['name']}, {p['country']}: {num(c['temperature_2m'])}°C, wind {num(c['wind_speed_10m'])} km/h"

    def wikipedia(args):
        topic = args.get("topic", "")
        ok, j = fetcher(f"https://en.wikipedia.org/api/rest_v1/page/summary/{_enc(topic.replace(' ', '_'))}", {"user-agent": "agent-02-tool-calling (github.com/braylo777)"})
        return ((j or {}).get("extract") or "")[:600] or "No summary" if ok else f"No article for {topic}"

    return {
        "calculator": lambda a: fmt(calc(a.get("expression", ""))),
        "get_weather": get_weather,
        "wikipedia": wikipedia,
    }


def tool_calls(r) -> list:
    """Normalize tool calls: native tool_calls, or JSON emitted in the text by some models."""
    raw = (r or {}).get("tool_calls")
    if not raw and (r or {}).get("response") is not None:
        v = r["response"]
        if isinstance(v, str):
            m = re.search(r"[\[{][\s\S]*[\]}]", v)
            try:
                v = json.loads(m.group(0)) if m else None
            except ValueError:
                v = None
        if v and not isinstance(v, list):
            v = [v]
        out = []
        for x in (v or []):
            if isinstance(x, str):
                try:
                    x = json.loads(x)
                except ValueError:
                    x = None
            out.append(x)
        raw = out
    calls = []
    for c in raw or []:
        if not isinstance(c, dict) or not c.get("name"):
            continue
        a = c["arguments"] if c.get("arguments") is not None else (c["parameters"] if c.get("parameters") is not None else {})
        if isinstance(a, str):
            try:
                a = json.loads(a or "{}")
            except ValueError:
                a = {}
        calls.append({"name": c["name"], "arguments": a})
    return calls


def run_tool_agent(ai, question: str, tools, max_steps: int = 4) -> dict:
    """ai(model, input) -> dict (Workers AI shape). Injected so tests run offline."""
    chat = lambda msgs, with_tools=False: ai(MODEL, {"messages": msgs, "max_tokens": 400, **({"tools": TOOL_DEFS} if with_tools else {})})
    messages = [{"role": "system", "content": SYSTEM}, {"role": "user", "content": question}]
    trace, seen = [], set()
    for _ in range(max_steps):
        r = chat(messages, True)
        calls = tool_calls(r)
        if not calls:
            return {"answer": text(r), "trace": trace}
        key = lambda c: c["name"] + js_json(c["arguments"])
        if all(key(c) in seen for c in calls):
            messages.append({"role": "user", "content": "You already have all tool results above. Answer the original question now."})
            return {"answer": text(chat(messages)), "trace": trace}
        for c in calls:
            seen.add(key(c))
        messages.append({"role": "assistant", "content": js_json(calls)})
        for c in calls:
            fn = tools.get(c["name"])
            try:
                out = fn(c["arguments"]) if fn else f"unknown tool {c['name']}"
            except CalcError as e:
                out = f"error: {e}"
            trace.append({"tool": c["name"], "args": c["arguments"], "result": out})
        messages.append({"role": "user", "content": "Tool results:\n" + "\n".join(f"{t['tool']}: {t['result']}" for t in trace[-len(calls):]) + "\nNow answer the original question. Do not call tools again unless needed."})
    return {"answer": "Step limit reached.", "trace": trace}

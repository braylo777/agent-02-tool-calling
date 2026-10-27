# agent-02-tool-calling

An AI agent that uses **native function calling** to choose and run real tools, then answers from their results. It runs serverless on Cloudflare Workers with the free Workers AI tier. Project 2 of a 10-part agentic AI portfolio.

**Live demo:** https://agent-02-tool-calling.braylo777.workers.dev

## What it is
Ask a question in plain English. The model decides which tools to call (`calculator`, `get_weather`, `wikipedia`) and with what arguments. The Worker runs them and the model writes the final answer. The response includes the full tool trace.

## How it works
```
question ─▶ LLM + JSON-schema tool defs
               │ tool_calls: [get_weather{city}, calculator{expression}]
               ▼
          Worker runs tools (Open-Meteo, Wikipedia REST, safe parser)
               │ role:"tool" results appended
               ▼
          LLM ─▶ final answer
```
- `src/agent.js`: tool schemas, tool implementations and the call loop (max 4 rounds).
- `src/calc.js`: safe arithmetic parser (no eval).
- Free, keyless APIs: Open-Meteo and Wikipedia REST.
- Guardrails: 5 requests/min per IP, 500-char input cap, tool errors are returned to the model instead of crashing.

## Run it
```bash
npm install
npm test            # offline unit tests (Node 20+), LLM is mocked
npx wrangler dev    # local; Workers AI runs via your Cloudflare login
npx wrangler deploy # free tier, no paid API keys
```

## Stack
Cloudflare Workers · Workers AI (`@cf/meta/llama-3.3-70b-instruct-fp8-fast`, function calling · Open-Meteo · Wikipedia REST) · vanilla JS · node:test

## License
MIT

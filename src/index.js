import { json, limited, body, cap } from "./core.js";
import { runToolAgent } from "./agent.js";
import page from "./page.html";

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    if (req.method === "POST" && url.pathname === "/api/ask") {
      if (await limited(req, env)) return json({ error: "Rate limit: try again in a minute." }, 429);
      const { question } = await body(req);
      if (!cap(question)) return json({ error: "question required (<=500 chars)" }, 400);
      try { return json(await runToolAgent(env, question)); } catch (e) { return json({ error: "Agent error: " + e.message }, 502); }
    }
    return new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } });
  },
};

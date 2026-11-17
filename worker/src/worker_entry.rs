//! Cloudflare Worker entry (wasm32 only). Same routes and JSON as the old src/index.js.
use crate::agent::{run_tool_agent, Ai, Fetcher, HttpResp};
use crate::{core::cap, page::PAGE};
use serde_json::{json, Value};
use wasm_bindgen::JsCast;
use worker::*;

struct CfAi(worker::Ai);
impl Ai for CfAi {
    async fn run(&self, model: &str, input: Value) -> std::result::Result<Value, String> {
        self.0.run::<Value, Value>(model, input).await.map_err(|e| e.to_string())
    }
}

struct Net;
impl Fetcher for Net {
    async fn get(&self, url: &str, user_agent: Option<&str>) -> std::result::Result<HttpResp, String> {
        let mut init = RequestInit::new();
        if let Some(ua) = user_agent {
            let mut h = Headers::new();
            h.set("user-agent", ua).map_err(|e| e.to_string())?;
            init.with_headers(h);
        }
        let req = Request::new_with_init(url, &init).map_err(|e| e.to_string())?;
        let mut r = Fetch::Request(req).send().await.map_err(|e| e.to_string())?;
        let ok = (200..300).contains(&r.status_code());
        let json = r.json::<Value>().await.unwrap_or(json!({}));
        Ok(HttpResp { ok, json })
    }
}

async fn rate_ok(env: &Env, ip: &str) -> bool {
    let rl = match js_sys::Reflect::get(env.as_ref(), &"RATE_LIMITER".into()) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return true,
    };
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"key".into(), &ip.into());
    let Ok(f) = js_sys::Reflect::get(&rl, &"limit".into()).and_then(|f| f.dyn_into::<js_sys::Function>().map_err(|e| e)) else { return true };
    let Ok(p) = f.call1(&rl, &opts) else { return true };
    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await {
        Ok(r) => js_sys::Reflect::get(&r, &"success".into()).ok().and_then(|v| v.as_bool()).unwrap_or(true),
        Err(_) => true,
    }
}

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    if req.method() == Method::Post && url.path() == "/api/ask" {
        let ip = req.headers().get("cf-connecting-ip")?.unwrap_or_else(|| "anon".into());
        if !rate_ok(&env, &ip).await {
            return Ok(Response::from_json(&json!({"error":"Rate limit: try again in a minute."}))?.with_status(429));
        }
        let body: Value = req.json().await.unwrap_or(json!({}));
        let q = body.get("question").and_then(|v| v.as_str());
        if !cap(q, 500) {
            return Ok(Response::from_json(&json!({"error":"question required (<=500 chars)"}))?.with_status(400));
        }
        let ai = CfAi(env.ai("AI")?);
        return match run_tool_agent(&ai, &Net, q.unwrap(), 4).await {
            Ok(v) => Response::from_json(&v),
            Err(e) => Ok(Response::from_json(&json!({"error": format!("Agent error: {e}")}))?.with_status(502)),
        };
    }
    let h = Headers::new();
    h.set("content-type", "text/html;charset=utf-8")?;
    Ok(Response::ok(PAGE)?.with_headers(h))
}

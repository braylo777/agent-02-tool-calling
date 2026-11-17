//! Tool-calling agent: model picks tools (JSON schema) -> we run them -> results go back -> model answers.
use crate::core::{calc, enc, fmt, text, MODEL};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;

pub const SYSTEM: &str = "You are a helpful assistant. Use a tool only when it helps; for general knowledge, greetings or translation, answer directly without tools. Answer concisely.";

pub fn tool_defs() -> Value {
    json!([
      {"name":"calculator","description":"Evaluate an arithmetic expression","parameters":{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}},
      {"name":"get_weather","description":"Current weather for a city","parameters":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}},
      {"name":"wikipedia","description":"Short Wikipedia summary of a topic","parameters":{"type":"object","properties":{"topic":{"type":"string"}},"required":["topic"]}}
    ])
}

pub struct HttpResp {
    pub ok: bool,
    pub json: Value,
}

/// Injected so tests run offline (the Worker uses the Fetch API).
#[allow(async_fn_in_trait)]
pub trait Fetcher {
    async fn get(&self, url: &str, user_agent: Option<&str>) -> Result<HttpResp, String>;
}

/// Injected Workers AI: run(model, input) -> response JSON.
#[allow(async_fn_in_trait)]
pub trait Ai {
    async fn run(&self, model: &str, input: Value) -> Result<Value, String>;
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}
fn n(v: &Value) -> String {
    fmt(v.as_f64().unwrap_or(f64::NAN))
}

pub async fn run_tool<F: Fetcher>(f: &F, name: &str, args: &Value) -> String {
    match name {
        "calculator" => match calc(s(args, "expression")) {
            Ok(v) => fmt(v),
            Err(e) => format!("error: {e}"),
        },
        "get_weather" => {
            let city = s(args, "city");
            let g = match f.get(&format!("https://geocoding-api.open-meteo.com/v1/search?count=1&name={}", enc(city)), None).await {
                Ok(r) => r.json,
                Err(e) => return format!("error: {e}"),
            };
            let Some(p) = g.get("results").and_then(|r| r.get(0)) else {
                return format!("No city named {city}");
            };
            let url = format!("https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,wind_speed_10m", p["latitude"], p["longitude"]);
            match f.get(&url, None).await {
                Ok(w) => {
                    let c = &w.json["current"];
                    format!("{}, {}: {}°C, wind {} km/h", s(p, "name"), s(p, "country"), n(&c["temperature_2m"]), n(&c["wind_speed_10m"]))
                }
                Err(e) => format!("error: {e}"),
            }
        }
        "wikipedia" => {
            let topic = s(args, "topic");
            let url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", enc(&topic.replace(' ', "_")));
            match f.get(&url, Some("agent-02-tool-calling (github.com/braylo777)")).await {
                Ok(r) if r.ok => {
                    let ex: String = s(&r.json, "extract").chars().take(600).collect();
                    if ex.is_empty() { "No summary".into() } else { ex }
                }
                Ok(_) => format!("No article for {topic}"),
                Err(e) => format!("error: {e}"),
            }
        }
        other => format!("unknown tool {other}"),
    }
}

/// Normalize tool calls: native `tool_calls`, or JSON emitted in the text by some models.
pub fn tool_calls(r: &Value) -> Vec<Value> {
    let mut raw: Vec<Value> = r.get("tool_calls").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if raw.is_empty() {
        if let Some(resp) = r.get("response").filter(|v| !v.is_null()) {
            let mut v = resp.clone();
            if let Value::String(st) = &v {
                let re = Regex::new(r"[\[{][\s\S]*[\]}]").unwrap();
                v = re.find(st).and_then(|m| serde_json::from_str(m.as_str()).ok()).unwrap_or(Value::Null);
            }
            let list = match v {
                Value::Null | Value::Bool(false) => vec![],
                Value::Array(a) => a,
                other => vec![other],
            };
            raw = list
                .into_iter()
                .map(|x| match x {
                    Value::String(st) => serde_json::from_str(&st).unwrap_or(Value::Null),
                    o => o,
                })
                .collect();
        }
    }
    raw.into_iter()
        .filter(|c| c.get("name").map(|n| n.as_str().map_or(!n.is_null(), |s| !s.is_empty())).unwrap_or(false))
        .map(|c| {
            let a = match c.get("arguments").filter(|v| !v.is_null()).or_else(|| c.get("parameters").filter(|v| !v.is_null())) {
                Some(a) => a.clone(),
                None => json!({}),
            };
            let a = match a {
                Value::String(st) => serde_json::from_str(if st.is_empty() { "{}" } else { &st }).unwrap_or(json!({})),
                o => o,
            };
            json!({"name": c["name"], "arguments": a})
        })
        .collect()
}

pub async fn run_tool_agent<A: Ai, F: Fetcher>(ai: &A, f: &F, question: &str, max_steps: usize) -> Result<Value, String> {
    let chat = |msgs: &Vec<Value>, with_tools: bool| {
        let mut inp = json!({"messages": msgs, "max_tokens": 400});
        if with_tools {
            inp["tools"] = tool_defs();
        }
        ai.run(MODEL, inp)
    };
    let mut messages = vec![json!({"role":"system","content":SYSTEM}), json!({"role":"user","content":question})];
    let mut trace: Vec<Value> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..max_steps {
        let r = chat(&messages, true).await?;
        let calls = tool_calls(&r);
        if calls.is_empty() {
            return Ok(json!({"answer": text(&r), "trace": trace}));
        }
        let key = |c: &Value| format!("{}{}", c["name"].as_str().unwrap_or(""), c["arguments"]);
        if calls.iter().all(|c| seen.contains(&key(c))) {
            messages.push(json!({"role":"user","content":"You already have all tool results above. Answer the original question now."}));
            let r = chat(&messages, false).await?;
            return Ok(json!({"answer": text(&r), "trace": trace}));
        }
        for c in &calls {
            seen.insert(key(c));
        }
        messages.push(json!({"role":"assistant","content": Value::Array(calls.clone()).to_string()}));
        let start = trace.len();
        for c in &calls {
            let name = c["name"].as_str().unwrap_or("");
            let out = run_tool(f, name, &c["arguments"]).await;
            trace.push(json!({"tool": name, "args": c["arguments"], "result": out}));
        }
        let lines: Vec<String> = trace[start..].iter().map(|t| format!("{}: {}", t["tool"].as_str().unwrap(), t["result"].as_str().unwrap())).collect();
        messages.push(json!({"role":"user","content": format!("Tool results:\n{}\nNow answer the original question. Do not call tools again unless needed.", lines.join("\n"))}));
    }
    Ok(json!({"answer":"Step limit reached.","trace":trace}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::cell::RefCell;

    struct MockAi(RefCell<Vec<Value>>, RefCell<Vec<bool>>);
    impl Ai for MockAi {
        async fn run(&self, _m: &str, input: Value) -> Result<Value, String> {
            self.1.borrow_mut().push(input.get("tools").is_some());
            let mut q = self.0.borrow_mut();
            Ok(if q.is_empty() { json!({"tool_calls":[{"name":"calculator","arguments":{"expression":"1+1"}}]}) } else { q.remove(0) })
        }
    }
    fn ai(r: Vec<Value>) -> MockAi {
        MockAi(RefCell::new(r), RefCell::new(vec![]))
    }
    struct Http(Box<dyn Fn(&str) -> Result<HttpResp, String>>, RefCell<Vec<String>>);
    impl Fetcher for Http {
        async fn get(&self, url: &str, _ua: Option<&str>) -> Result<HttpResp, String> {
            self.1.borrow_mut().push(url.to_string());
            (self.0)(url)
        }
    }
    fn http(f: impl Fn(&str) -> Result<HttpResp, String> + 'static) -> Http {
        Http(Box::new(f), RefCell::new(vec![]))
    }
    fn ok(j: Value) -> Result<HttpResp, String> {
        Ok(HttpResp { ok: true, json: j })
    }

    #[test]
    fn calc_and_reject_code() {
        assert_eq!(calc("2+3*(4-1)"), Ok(11.0));
        assert!(calc("alert(1)").is_err());
        assert!(calc("2**").is_err());
        assert_eq!(block_on(run_tool(&http(|_| ok(json!({}))), "calculator", &json!({"expression":"0.18*240"}))), "43.2");
    }
    #[test]
    fn tool_loop_native_calls() {
        let a = ai(vec![json!({"tool_calls":[{"name":"calculator","arguments":{"expression":"6*7"}}]}), json!({"response":"42"})]);
        let r = block_on(run_tool_agent(&a, &http(|_| ok(json!({}))), "6*7?", 4)).unwrap();
        assert_eq!(r["answer"], "42");
        assert_eq!(r["trace"][0]["result"], "42");
    }
    #[test]
    fn tool_calls_text_and_string_args() {
        let r = json!({"response":["{\"type\":\"function\",\"name\":\"calculator\",\"parameters\":{\"expression\":\"2+2\"}}"]});
        assert_eq!(tool_calls(&r), vec![json!({"name":"calculator","arguments":{"expression":"2+2"}})]);
        assert!(tool_calls(&json!({"response":"Hello there"})).is_empty());
        assert_eq!(tool_calls(&json!({"tool_calls":[{"name":"wikipedia","arguments":"{\"topic\":\"Rust\"}"}]})), vec![json!({"name":"wikipedia","arguments":{"topic":"Rust"}})]);
        assert_eq!(tool_calls(&json!({"tool_calls":[{"name":"calculator","arguments":"{oops"},{"arguments":{}}]})), vec![json!({"name":"calculator","arguments":{}})]);
        assert_eq!(tool_calls(&json!({"response":"Sure {\"name\":\"get_weather\",\"arguments\":{\"city\":\"Oslo\"}}"})), vec![json!({"name":"get_weather","arguments":{"city":"Oslo"}})]);
    }
    #[test]
    fn weather_mocked_and_unknown_city() {
        let h = http(|u| if u.contains("geocoding") { ok(json!({"results":[{"name":"Austin","country":"US","latitude":1,"longitude":2}]})) } else { ok(json!({"current":{"temperature_2m":20,"wind_speed_10m":5}})) });
        assert_eq!(block_on(run_tool(&h, "get_weather", &json!({"city":"Austin"}))), "Austin, US: 20°C, wind 5 km/h");
        let h = http(|_| ok(json!({"results":[]})));
        assert_eq!(block_on(run_tool(&h, "get_weather", &json!({"city":"Nowhere"}))), "No city named Nowhere");
    }
    #[test]
    fn wikipedia_cases() {
        let h = http(|_| ok(json!({"extract":"x".repeat(900)})));
        assert_eq!(block_on(run_tool(&h, "wikipedia", &json!({"topic":"A B"}))).len(), 600);
        assert!(h.1.borrow()[0].ends_with("A_B"));
        let nf = http(|_| Ok(HttpResp { ok: false, json: json!({}) }));
        assert_eq!(block_on(run_tool(&nf, "wikipedia", &json!({"topic":"Zzz"}))), "No article for Zzz");
    }
    #[test]
    fn parallel_calls_unknown_tool() {
        let a = ai(vec![json!({"tool_calls":[{"name":"calculator","arguments":{"expression":"2+2"}},{"name":"nope","arguments":{}}]}), json!({"response":"done"})]);
        let r = block_on(run_tool_agent(&a, &http(|_| ok(json!({}))), "q", 4)).unwrap();
        assert_eq!(r["trace"][0]["result"], "4");
        assert_eq!(r["trace"][1]["result"], "unknown tool nope");
        assert_eq!(r["answer"], "done");
    }
    #[test]
    fn step_limit_when_model_keeps_calling() {
        // mock repeats the same call forever: loop guard would trigger, so vary expressions
        let calls: Vec<Value> = (0..5).map(|k| json!({"tool_calls":[{"name":"calculator","arguments":{"expression":format!("{k}+1")}}]})).collect();
        let r = block_on(run_tool_agent(&ai(calls), &http(|_| ok(json!({}))), "q", 2)).unwrap();
        assert_eq!(r["answer"], "Step limit reached.");
        assert_eq!(r["trace"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn loop_guard_tool_free_final_answer() {
        let call = json!({"tool_calls":[{"name":"calculator","arguments":{"expression":"1+1"}}]});
        let a = ai(vec![call.clone(), call, json!({"response":"2"})]);
        let r = block_on(run_tool_agent(&a, &http(|_| ok(json!({}))), "q", 4)).unwrap();
        assert_eq!(r["answer"], "2");
        assert_eq!(r["trace"].as_array().unwrap().len(), 1);
        assert_eq!(*a.1.borrow(), vec![true, true, false]);
    }
    #[test]
    fn tool_defs_valid() {
        let d = tool_defs();
        let names: Vec<&str> = d.as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["calculator", "get_weather", "wikipedia"]);
    }
}

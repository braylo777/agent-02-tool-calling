//! Shared helpers: JSON text, safe calculator, input cap, URL encoding.
use regex::Regex;
use serde_json::Value;

pub const MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";

pub fn text(r: &Value) -> String {
    match r.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => r.to_string(),
        Some(v) => v.to_string(),
    }
}

pub fn cap(s: Option<&str>, n: usize) -> bool {
    matches!(s, Some(s) if !s.trim().is_empty() && s.chars().count() <= n)
}

/// encodeURIComponent
pub fn enc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{:02X}", b));
        }
    }
    o
}

/// Format like JS String(+v.toPrecision(12)).
pub fn fmt(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let v: f64 = format!("{:.11e}", v).parse().unwrap_or(v);
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if a < 1e-6 || a >= 1e21 {
        let s = format!("{:e}", v);
        return if a >= 1e21 { s.replace('e', "e+") } else { s };
    }
    format!("{}", v)
}

const ERR: &str = "invalid expression";

pub fn calc(expr: &str) -> Result<f64, String> {
    let src: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let re = Regex::new(r"\d+\.?\d*|[-+*/%()]").unwrap();
    let t: Vec<&str> = re.find_iter(&src).map(|m| m.as_str()).collect();
    if src.is_empty() || t.concat() != src {
        return Err(ERR.into());
    }
    let mut p = P { t, i: 0 };
    let v = p.add()?;
    if p.i != p.t.len() {
        return Err(ERR.into());
    }
    Ok(v)
}

struct P<'a> {
    t: Vec<&'a str>,
    i: usize,
}
impl<'a> P<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.t.get(self.i).copied()
    }
    fn num(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some("-") => {
                self.i += 1;
                Ok(-self.num()?)
            }
            Some("(") => {
                self.i += 1;
                let v = self.add()?;
                if self.peek() != Some(")") {
                    return Err(ERR.into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(tok) => {
                self.i += 1;
                tok.parse::<f64>().map_err(|_| ERR.to_string())
            }
            None => Err(ERR.into()),
        }
    }
    fn mul(&mut self) -> Result<f64, String> {
        let mut v = self.num()?;
        while let Some(o @ ("*" | "/" | "%")) = self.peek() {
            self.i += 1;
            let r = self.num()?;
            v = match o {
                "*" => v * r,
                "/" => v / r,
                _ => v % r,
            };
        }
        Ok(v)
    }
    fn add(&mut self) -> Result<f64, String> {
        let mut v = self.mul()?;
        while let Some(o @ ("+" | "-")) = self.peek() {
            self.i += 1;
            let r = self.mul()?;
            v = if o == "+" { v + r } else { v - r };
        }
        Ok(v)
    }
}

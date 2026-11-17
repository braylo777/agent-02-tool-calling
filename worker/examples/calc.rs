use agent_02_tool_calling::core::{calc, fmt};
fn main() {
    let ex: Vec<String> = serde_json::from_str(&std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let out: Vec<String> = ex.iter().map(|e| match calc(e) { Ok(v) => fmt(v), Err(m) => format!("error: {m}") }).collect();
    println!("{}", serde_json::to_string(&out).unwrap());
}

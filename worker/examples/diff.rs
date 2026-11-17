// Differential check helper: prints tool_calls() for each fixture in a JSON array.
use agent_02_tool_calling::agent::tool_calls;
fn main() {
    let p = std::env::args().nth(1).expect("path");
    let fx: Vec<serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let out: Vec<_> = fx.iter().map(tool_calls).collect();
    println!("{}", serde_json::to_string(&out).unwrap());
}

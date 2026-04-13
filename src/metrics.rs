use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct RoundMetrics {
    pub timestamp: String,
    pub session_id: String,
    pub round: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub wall_ms: u64,
    pub tool_calls: u32,
    pub retries: u32,
    pub host: &'static str,
}

pub fn emit(path: &Path, m: &RoundMetrics) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", serde_json::to_string(m).unwrap())?;
    Ok(())
}

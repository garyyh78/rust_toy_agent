use crate::agent_loop::{agent_loop, Messages};
use crate::client::AnthropicClient;
use crate::logger::SessionLogger;
use crate::todo_manager::TodoManager;
use crate::tools::TOOLS;
use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub prompt: String,
    pub expected_output: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub model: String,
    pub commit: String,
    pub test_time: String,
    pub passed: bool,
    pub steps: u32,
    pub actual_output: String,
    pub expected_output: String,
    pub total_time_ms: u64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub fn get_git_commit() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output();
    
    match output {
        Ok(o) => {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout).trim().to_string()
            } else {
                "unknown".to_string()
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

pub fn get_test_timestamp() -> String {
    Local::now().format("%Y%m%d_%H%M%S").to_string()
}

fn extract_final_text(messages: &[Json]) -> String {
    let mut text = String::new();
    if let Some(last) = messages.last() {
        if let Some(blocks) = last["content"].as_array() {
            for block in blocks {
                if block["type"] == "text" {
                    if let Some(t) = block["text"].as_str() {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(t);
                    }
                }
            }
        }
    }
    text
}

pub async fn run_test(
    client: &AnthropicClient,
    model: &str,
    test_case: &TestCase,
    workdir: &Path,
    todo: &Arc<Mutex<TodoManager>>,
    logger: &mut SessionLogger,
) -> TestResult {
    let start_time = Instant::now();
    let commit = get_git_commit();
    let test_time = get_test_timestamp();

    let system = format!(
        "You are a coding agent at {}. \
Use the todo tool to plan multi-step tasks. Mark in_progress before starting, completed when done. \
Prefer tools over prose.",
        workdir.display()
    );

    let tools: Json = serde_json::from_str(TOOLS).unwrap();
    let mut history: Messages = vec![serde_json::json!({"role": "user", "content": test_case.prompt})];

    let (total_input_tokens, total_output_tokens, steps) = agent_loop(
        client,
        model,
        &system,
        &tools,
        &mut history,
        workdir,
        todo,
        logger,
    )
    .await;

    let response_text = extract_final_text(&history);

    let elapsed = start_time.elapsed();
    let total_time_ms = elapsed.as_millis() as u64;

    let actual_output = response_text.trim().to_string();
    let expected_output = test_case.expected_output.trim().to_string();

    let passed = actual_output.contains(&expected_output) || actual_output == expected_output;

    TestResult {
        name: test_case.name.clone(),
        model: model.to_string(),
        commit,
        test_time,
        passed,
        steps,
        actual_output,
        expected_output,
        total_time_ms,
        total_tokens: total_input_tokens + total_output_tokens,
        input_tokens: total_input_tokens,
        output_tokens: total_output_tokens,
    }
}

pub fn load_test_case(path: &Path) -> Result<TestCase, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read test file: {}", e))?;
    
    let json: Json = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse test JSON: {}", e))?;

    let name = json["name"]
        .as_str()
        .ok_or("Missing 'name' field")?
        .to_string();
    let prompt = json["prompt"]
        .as_str()
        .ok_or("Missing 'prompt' field")?
        .to_string();
    let expected_output = json["expected_output"]
        .as_str()
        .ok_or("Missing 'expected_output' field")?
        .to_string();
    let language = json["language"]
        .as_str()
        .map(String::from);

    Ok(TestCase {
        name,
        prompt,
        expected_output,
        language,
    })
}

pub fn load_test_from_dir(dir_path: &Path) -> Result<TestCase, String> {
    let test_file = dir_path.join("test.json");
    load_test_case(&test_file)
}

pub fn print_test_result(result: &TestResult) {
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Test: {}", result.name);
    println!("═══════════════════════════════════════════════════════════════");
    
    if result.passed {
        println!("  ✓ PASSED");
    } else {
        println!("  ✗ FAILED");
    }
    
    println!();
    println!("  Model: {}", result.model);
    println!("  Commit: {}", result.commit);
    println!("  Time: {} ms", result.total_time_ms);
    println!("  Tokens: {} ({} in / {} out)", result.total_tokens, result.input_tokens, result.output_tokens);
    println!();
    
    if !result.passed {
        println!("  Expected output:");
        println!("  ────────────────────────────────────────────────────────────");
        println!("  {}", result.expected_output);
        println!();
        println!("  Actual output:");
        println!("  ────────────────────────────────────────────────────────────");
        println!("  {}", result.actual_output);
        println!();
    }
}

pub fn save_test_result(result: &TestResult, results_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(results_dir)?;
    
    let filename = format!("{}_{}.json", result.name, result.test_time);
    let filepath = results_dir.join(&filename);
    
    let json = serde_json::to_string_pretty(result)?;
    fs::write(filepath, json)?;
    
    Ok(())
}
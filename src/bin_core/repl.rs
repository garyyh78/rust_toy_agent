use crate::bin_core::agent_loop::agent_loop;
use crate::bin_core::state::State;
use serde_json::Value as Json;
use std::io::{BufRead, Write};

/// Read a prompt from stdin.
pub fn read_prompt(prompt: &str) -> Option<String> {
    print!("{prompt}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line.trim().to_string()),
    }
}

/// Extract the final text from the last assistant message.
pub fn extract_final_text(messages: &[Json]) -> String {
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

/// Print help message.
pub fn print_usage() {
    eprintln!("Usage: rust_toy_agent [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --test <test_name>    Run in end2end test mode with the specified test");
    eprintln!("  -h, --help            Show this help message");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  rust_toy_agent                    # Start in interactive REPL mode");
    eprintln!("  rust_toy_agent --test sum_1_to_n  # Run the sum_1_to_n test");
}

/// Run the interactive REPL.
pub async fn run_repl(state: State) {
    let system = format!(
        "You are a coding agent at {}. \
         Use tools to solve tasks. Prefer task_create/task_update/task_list for multi-step work. \
         Use TodoWrite for short checklists. Use task for subagent delegation. \
         Use load_skill for specialized knowledge.\n\
         Skills: {}",
        state.workdir.display(),
        state.skills.get_descriptions()
    );

    eprintln!();
    eprintln!("\x1b[35m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[35m║          Full Agent - All Mechanisms Edition                ║\x1b[0m");
    eprintln!("\x1b[35m╚══════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
    eprintln!("  Model: {}", state.model);
    eprintln!("  Workdir: {}", state.workdir.display());
    eprintln!("  Tools: 23 (bash, read, write, edit, TodoWrite, task, load_skill,");
    eprintln!("           compact, bg_run, bg_check, task CRUD, team, messaging,");
    eprintln!("           broadcast, shutdown, plan, idle, claim)");
    eprintln!("\x1b[34m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    eprintln!();

    let mut history: Vec<Json> = Vec::new();

    loop {
        let query = match read_prompt("\x1b[36mfull >> \x1b[0m") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
            eprintln!("\x1b[35m  Session ended.\x1b[0m");
            break;
        }

        match query.as_str() {
            "/compact" => {
                if !history.is_empty() {
                    eprintln!("[manual compact via /compact]");
                    history = state.compactor.auto_compact(&history).await;
                }
                continue;
            }
            "/tasks" => {
                let mgr = state.task_mgr.lock().unwrap();
                println!("{}", mgr.list_all());
                continue;
            }
            "/team" => {
                let team = state.team.lock().unwrap();
                println!("{}", team.list_all());
                continue;
            }
            "/inbox" => {
                let msgs = state.bus.read_inbox("lead");
                println!(
                    "{}",
                    serde_json::to_string_pretty(&msgs).unwrap_or_default()
                );
                continue;
            }
            _ => {}
        }

        eprintln!();
        let preview = if query.chars().count() > 50 {
            format!("{}...", query.chars().take(50).collect::<String>())
        } else {
            query.clone()
        };
        eprintln!("\x1b[35m  Turn {}\x1b[0m", preview);
        eprintln!();

        history.push(serde_json::json!({"role": "user", "content": query}));
        agent_loop(&state, &mut history, &system).await;

        let response_text = extract_final_text(&history);
        println!("{response_text}");
        println!();
    }
}

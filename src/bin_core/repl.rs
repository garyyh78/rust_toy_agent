use crate::agent_loop::extract_final_text;
use crate::bin_core::agent_loop::agent_loop;
use crate::bin_core::constants::LEAD;
use crate::bin_core::state::State;
use serde_json::Value as Json;
use std::io::{BufRead, Write};
use std::path::PathBuf;

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

/// Print help message.
pub fn print_usage() {
    tracing::info!("Usage: rust_toy_agent [OPTIONS]");
    tracing::info!("");
    tracing::info!("Options:");
    tracing::info!("  --test <test_name>    Run in end2end test mode with the specified test");
    tracing::info!("  --metrics-out <path> Write metrics to the specified file (ndjson format)");
    tracing::info!("  -h, --help            Show this help message");
    tracing::info!("");
    tracing::info!("Examples:");
    tracing::info!("  rust_toy_agent                    # Start in interactive REPL mode");
    tracing::info!("  rust_toy_agent --test sum_1_to_n  # Run the sum_1_to_n test");
    tracing::info!("  rust_toy_agent --metrics-out metrics.ndjson  # Write metrics to file");
}

/// Run the interactive REPL.
pub async fn run_repl(state: State, metrics_out: Option<PathBuf>) {
    let session_id = uuid::Uuid::new_v4().to_string();
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
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║          Full Agent - All Mechanisms Edition                ║");
    eprintln!("╚══════════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("  Model: {}", state.model);
    eprintln!("  Workdir: {}", state.workdir.display());
    eprintln!("  Tools: 23 (bash, read, write, edit, TodoWrite, task, load_skill,");
    eprintln!("           compact, bg_run, bg_check, task CRUD, team, messaging,");
    eprintln!("           broadcast, shutdown, plan, idle, claim)");
    eprintln!("  Session: {}", session_id);
    eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    eprintln!();

    let mut history: Vec<Json> = Vec::new();

    loop {
        let query = match read_prompt("full >> ") {
            None => break,
            Some(q) => q,
        };
        if matches!(query.as_str(), "q" | "exit" | "") {
            tracing::info!("Session ended");
            break;
        }

        match query.as_str() {
            "/compact" => {
                if !history.is_empty() {
                    tracing::info!("manual compact via /compact");
                    history = state.compactor.auto_compact(&history).await;
                }
                continue;
            }
            "/tasks" => {
                let mgr = match state.task_mgr.lock() {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!(error = %e, "lock poisoned");
                        continue;
                    }
                };
                println!("{}", mgr.list_all());
                continue;
            }
            "/team" => {
                let team = match state.team.lock() {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::error!(error = %e, "lock poisoned");
                        continue;
                    }
                };
                println!("{}", team.list_all());
                continue;
            }
            "/inbox" => {
                let msgs = state.bus.read_inbox(LEAD);
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
        tracing::info!(query = %preview, "Turn");
        eprintln!();

        history.push(serde_json::json!({"role": "user", "content": query}));
        agent_loop(
            &state,
            &mut history,
            &system,
            metrics_out.as_ref(),
            &session_id,
        )
        .await;

        let response_text = extract_final_text(&history);
        println!("{response_text}");
        println!();
    }
}

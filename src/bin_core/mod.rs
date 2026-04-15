//! `bin_core` - Core binary functionality
//!
//! This module contains all the functionality that was previously in main.rs,
//! organized into logical submodules:
//!
//! - constants: Application constants
//! - state: The State struct and its implementation
//! - dispatch: Tool dispatch logic
//! - `agent_loop`: Main agent loop
//! - teammate: Teammate agent loop
//! - repl: Interactive REPL
//! - `test_mode`: End-to-end test mode

pub mod agent_loop;
pub mod constants;
pub mod dispatch;
pub mod repl;
pub mod state;
pub mod teammate;
pub mod test_mode;

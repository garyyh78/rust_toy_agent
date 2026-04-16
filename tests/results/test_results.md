# Test Results

**Date:** 2026-04-16

## Summary

- **Total tests:** 212
- **Passed:** 212
- **Failed:** 0
- **Ignored:** 0

## Test Suites

### Unit Tests (lib)

- **Tests:** 205
- **Status:** All passed

| Test Name | Status |
|-----------|--------|
| agent_loop::tests::empty_content_array_is_ok | ✅ |
| agent_loop::tests::assistant_with_only_text_is_ok | ✅ |
| agent_loop::tests::content_as_string_not_array_is_ok | ✅ |
| agent_loop::tests::multi_tool_use_all_matched | ✅ |
| agent_loop::tests::mismatched_tool_use_id_fails | ✅ |
| agent_loop::tests::multi_tool_use_some_matched | ✅ |
| agent_loop::tests::test_corrupted_history_detection | ✅ |
| agent_loop::tests::test_extract_429_rate_limit_error | ✅ |
| agent_loop::tests::test_extract_error_message_fields | ✅ |
| agent_loop::tests::test_extract_401_auth_error | ✅ |
| agent_loop::tests::test_extract_final_text_empty | ✅ |
| agent_loop::tests::test_extract_final_text_mixed_content | ✅ |
| agent_loop::tests::test_extract_final_text_only | ✅ |
| agent_loop::tests::test_extract_400_tool_use_error | ✅ |
| agent_loop::tests::test_extract_final_text_tool_use_only | ✅ |
| agent_loop::tests::test_messages_append_flow | ✅ |
| agent_loop::tests::test_extract_non_json_error_returns_none | ✅ |
| agent_loop::tests::test_nag_reminder_appended_to_tool_result | ✅ |
| agent_loop::tests::test_nag_reminder_all_results_are_tool_result_type | ✅ |
| agent_loop::tests::test_multiple_tool_use_all_need_results | ✅ |
| agent_loop::tests::test_nag_reminder_resets_after_todo | ✅ |
| agent_loop::tests::test_nag_reminder_skipped_when_no_results | ✅ |
| agent_loop::tests::test_nag_reminder_threshold | ✅ |
| agent_loop::tests::test_tool_result_json_structure | ✅ |
| agent_loop::tests::test_stop_reason_handling | ✅ |
| agent_loop::tests::test_valid_tool_use_followed_by_tool_result | ✅ |
| agent_loop::tests::test_validate_empty_history_is_ok | ✅ |
| agent_loop::tests::test_validate_single_assistant_tool_use_is_ok | ✅ |
| agent_loop::tests::test_validate_single_user_message_is_ok | ✅ |
| agent_loop::tests::tool_result_out_of_order_is_actually_ok | ✅ |
| agent_loop::tests::trailing_tool_use_is_ok | ✅ |
| agent_loop::tests::test_truncate_preserves_initial_user_prompt | ✅ |
| agent_loop::tests::test_truncate_does_not_split_tool_pair | ✅ |
| agent_loop::tests::test_system_prompt_format | ✅ |
| agent_teams::tests::test_message_bus_all_msg_types | ✅ |
| agent_teams::tests::test_message_bus_send_and_read | ✅ |
| agent_teams::tests::test_message_creation | ✅ |
| agent_teams::tests::test_message_serialization | ✅ |
| agent_teams::tests::test_config_serialization_roundtrip | ✅ |
| agent_teams::tests::test_teammate_manager_new | ✅ |
| agent_teams::tests::test_teammate_manager_list_all | ✅ |
| agent_teams::tests::test_teammate_manager_persistence | ✅ |
| agent_teams::tests::test_teammate_manager_set_status | ✅ |
| agent_teams::tests::test_teammate_manager_spawn | ✅ |
| agent_teams::tests::test_teammate_manager_spawn_busy_rejected | ✅ |
| agent_teams::tests::test_teammate_manager_spawn_existing | ✅ |
| agent_teams::tests::test_message_bus_broadcast | ✅ |
| agent_teams::tests::test_message_bus_invalid_type | ✅ |
| bin_core::constants::tests::lead_constant_is_lowercase | ✅ |
| agent_teams::tests::test_message_bus_multiple_messages | ✅ |
| agent_loop::tests::prop_tests::corrupting_tool_use_id_is_caught | ✅ |
| agent_loop::tests::prop_tests::random_valid_history_passes | ✅ |
| agent_teams::tests::test_message_bus_read_empty_inbox | ✅ |
| bin_core::dispatch::tests::test_dispatch_compact | ✅ |
| bin_core::dispatch::tests::test_dispatch_worktree_create_missing_git | ✅ |
| bin_core::dispatch::tests::test_dispatch_idle | ✅ |
| bin_core::dispatch::tests::test_dispatch_unknown_tool | ✅ |
| context_compact::tests::test_context_compactor_creation | ✅ |
| bin_core::state::tests::test_state_creation | ✅ |
| context_compact::tests::test_estimate_tokens | ✅ |
| bin_core::state::tests::test_state_tools | ✅ |
| bin_core::dispatch::tests::test_dispatch_worktree_list | ✅ |
| context_compact::tests::test_micro_compact | ✅ |
| llm_client::tests::test_build_request_body_empty_system_omitted | ✅ |
| context_compact::tests::test_micro_compact_no_compress_recent | ✅ |
| llm_client::tests::test_build_request_body_empty_tools_omitted | ✅ |
| llm_client::tests::test_build_request_body_minimal | ✅ |
| llm_client::tests::test_build_request_body_with_system | ✅ |
| llm_client::tests::test_build_request_body_with_tools | ✅ |
| llm_client::tests::test_from_env_defaults | ✅ |
| llm_client::tests::test_new_with_args | ✅ |
| logger::tests::test_log_info_runs | ✅ |
| logger::tests::test_log_output_preview_long | ✅ |
| logger::tests::test_log_output_preview_short | ✅ |
| logger::tests::test_log_section_runs | ✅ |
| logger::tests::test_log_step_runs | ✅ |
| llm_client::tests::test_create_message_returns_err_on_bad_url | ✅ |
| logger::tests::test_session_logger_creates_file | ✅ |
| logger::tests::test_session_logger_creates_parent_dirs | ✅ |
| context_compact::tests::test_dispatch_tool | ✅ |
| logger::tests::test_session_logger_no_file_no_panic | ✅ |
| logger::tests::test_session_logger_stderr_only | ✅ |
| skill_loading::tests::test_skill_loader_empty | ✅ |
| logger::tests::test_session_logger_timestamp_format | ✅ |
| skill_loading::tests::test_skill_loader_no_frontmatter | ✅ |
| skill_loading::tests::test_skill_loader_parse_frontmatter | ✅ |
| subagent::tests::test_child_tools_excludes_todo | ✅ |
| skill_loading::tests::test_skill_loader_with_skills | ✅ |
| subagent::tests::test_child_tools_filter_handles_malformed_name | ✅ |
| subagent::tests::test_dispatch_bash_dangerous_blocked | ✅ |
| subagent::tests::test_dispatch_bash_missing_command | ✅ |
| subagent::tests::test_dispatch_bash | ✅ |
| subagent::tests::test_dispatch_edit_file_text_not_found | ✅ |
| subagent::tests::test_dispatch_edit_file | ✅ |
| subagent::tests::test_dispatch_read_file_missing_path | ✅ |
| subagent::tests::test_dispatch_read_file | ✅ |
| subagent::tests::test_dispatch_unknown_tool | ✅ |
| subagent::tests::test_dispatch_write_file | ✅ |
| subagent::tests::test_dispatch_write_file_missing_fields | ✅ |
| subagent::tests::test_extract_summary_empty_messages | ✅ |
| subagent::tests::test_extract_summary_from_text_blocks | ✅ |
| subagent::tests::test_extract_summary_no_text_blocks | ✅ |
| logger::tests::test_prune_old_logs_keeps_20 | ✅ |
| subagent::tests::test_subagent_new_with_different_workdir | ✅ |
| subagent::tests::test_subagent_creation | ✅ |
| task_system::tests::test_create_task | ✅ |
| task_system::tests::test_list_tasks | ✅ |
| team_protocols::tests::test_concurrent_plan_submissions | ✅ |
| team_protocols::tests::test_concurrent_shutdown_requests | ✅ |
| team_protocols::tests::test_create_shutdown_request | ✅ |
| task_system::tests::test_update_status | ✅ |
| team_protocols::tests::test_list_plan_requests | ✅ |
| team_protocols::tests::test_list_shutdown_requests | ✅ |
| team_protocols::tests::test_plan_request_serialization | ✅ |
| team_protocols::tests::test_protocol_tracker_creation | ✅ |
| team_protocols::tests::test_request_status_serialization | ✅ |
| team_protocols::tests::test_respond_shutdown_approve | ✅ |
| team_protocols::tests::test_respond_shutdown_reject | ✅ |
| team_protocols::tests::test_respond_shutdown_unknown_id | ✅ |
| team_protocols::tests::test_review_plan_approve | ✅ |
| team_protocols::tests::test_review_plan_no_feedback | ✅ |
| team_protocols::tests::test_review_plan_reject | ✅ |
| team_protocols::tests::test_review_plan_unknown_id | ✅ |
| team_protocols::tests::test_shutdown_request_serialization | ✅ |
| team_protocols::tests::test_submit_plan | ✅ |
| text_util::tests::truncate_chars_ellipsis_no_truncation | ✅ |
| text_util::tests::truncate_chars_ellipsis_truncated | ✅ |
| text_util::tests::truncate_chars_handles_ascii | ✅ |
| text_util::tests::truncate_chars_handles_emoji | ✅ |
| todo_manager::tests::test_basic_pending_and_in_progress | ✅ |
| todo_manager::tests::test_completed_items | ✅ |
| todo_manager::tests::test_default_is_empty | ✅ |
| todo_manager::tests::test_empty_list_clears_todos | ✅ |
| todo_manager::tests::test_empty_text_rejected | ✅ |
| todo_manager::tests::test_invalid_status_rejected | ✅ |
| todo_manager::tests::test_items_accessor | ✅ |
| todo_manager::tests::test_max_items_rejected | ✅ |
| todo_manager::tests::test_max_items_boundary_ok | ✅ |
| todo_manager::tests::test_missing_status_defaults_to_pending | ✅ |
| todo_manager::tests::test_missing_id_uses_index | ✅ |
| todo_manager::tests::test_mixed_statuses | ✅ |
| todo_manager::tests::test_multiple_in_progress_rejected | ✅ |
| todo_manager::tests::test_new_is_empty | ✅ |
| todo_manager::tests::test_render_format | ✅ |
| todo_manager::tests::test_update_replaces_previous_items | ✅ |
| todo_manager::tests::test_whitespace_text_rejected | ✅ |
| tool_runners::tests::test_normalize_curdir | ✅ |
| tool_runners::tests::test_normalize_multiple_parents | ✅ |
| tool_runners::tests::test_normalize_no_change | ✅ |
| tool_runners::tests::test_normalize_parentdir | ✅ |
| tool_runners::tests::symlink_outside_workdir_is_rejected | ✅ |
| tool_runners::tests::test_run_bash_dangerous_blocked | ✅ |
| tool_runners::tests::test_run_bash_no_output | ✅ |
| tool_runners::tests::bash_does_not_inherit_api_key | ✅ |
| tool_runners::tests::test_run_edit | ✅ |
| tool_runners::tests::test_run_edit_text_not_found | ✅ |
| tool_runners::tests::test_run_read_with_limit | ✅ |
| tool_runners::tests::test_run_write_and_read | ✅ |
| tool_runners::tests::test_safe_path_allowed | ✅ |
| tool_runners::tests::test_safe_path_escape_rejected | ✅ |
| tools::tests::test_bash_tool_schema | ✅ |
| tools::tests::test_compactor_tools_count | ✅ |
| tools::tests::test_core_file_tools_count | ✅ |
| tool_runners::tests::test_run_bash_captures_stderr | ✅ |
| tools::tests::test_dispatch_todo_error_returns_true | ✅ |
| tools::tests::test_dispatch_todo_tool | ✅ |
| tools::tests::test_dispatch_unknown_tool | ✅ |
| tools::tests::test_full_agent_tools_count | ✅ |
| tools::tests::test_skill_agent_tools_count | ✅ |
| tools::tests::test_teammate_tools_count | ✅ |
| tools::tests::test_todo_tool_schema | ✅ |
| tools::tests::test_tool_bash_schema | ✅ |
| tools::tests::test_tool_edit_file_schema | ✅ |
| tools::tests::test_tool_read_file_schema | ✅ |
| tools::tests::test_tool_result_structure | ✅ |
| tools::tests::test_tool_todo_schema | ✅ |
| tools::tests::test_tool_write_file_schema | ✅ |
| tools::tests::test_tools_json_parsing | ✅ |
| worktree::binding::tests::test_task_binding_bind | ✅ |
| worktree::binding::tests::test_task_binding_complete | ✅ |
| worktree::binding::tests::test_task_binding_not_found | ✅ |
| worktree::binding::tests::test_task_binding_unbind | ✅ |
| worktree::events::tests::test_event_bus_creation | ✅ |
| worktree::events::tests::test_event_bus_emit_and_list | ✅ |
| tool_runners::tests::test_run_bash_simple_echo | ✅ |
| worktree::events::tests::test_event_bus_limit | ✅ |
| worktree::index::tests::test_worktree_index_creation | ✅ |
| worktree::index::tests::test_worktree_index_add_and_find | ✅ |
| worktree::index::tests::test_worktree_index_duplicate_rejected | ✅ |
| worktree::index::tests::test_worktree_index_list_all | ✅ |
| worktree::index::tests::test_worktree_index_update_status | ✅ |
| worktree::manager::detect_repo_root_tests::test_detect_repo_root_in_repo | ✅ |
| worktree::manager::detect_repo_root_tests::test_detect_repo_root_not_in_repo | ✅ |
| worktree::manager::validate_name_tests::test_validate_name_invalid | ✅ |
| worktree::manager::validate_name_tests::test_validate_name_valid | ✅ |
| worktree::manager_tests::tests::test_worktree_entry_serialization | ✅ |
| worktree::manager_tests::tests::test_worktree_event_serialization | ✅ |
| worktree::manager_tests::tests::test_worktree_manager_creation | ✅ |
| worktree::manager_tests::tests::test_worktree_manager_no_git | ✅ |
| tools::tests::test_dispatch_bash_not_todo | ✅ |
| background_tasks::tests::test_background_task_creation | ✅ |
| llm_client::tests::test_create_message_returns_err_on_api_error | ✅ |
| llm_client::tests::test_create_message_err_contains_status_code | ✅ |
| background_tasks::tests::background_command_runs_exactly_once | ✅ |
| background_tasks::tests::test_drain_notifications | ✅ |

### CLI Integration Tests

- **Tests:** 4
- **Status:** All passed

| Test Name | Status |
|-----------|--------|
| help_prints_usage | ✅ |
| test_mode_missing_arg | ✅ |
| test_mode_unknown_name_errors | ✅ |
| repl_empty_stdin_exits_cleanly | ✅ |

### Mock LLM Integration Tests

- **Tests:** 3
- **Status:** All passed

| Test Name | Status |
|-----------|--------|
| help_works_with_no_api_key | ✅ |
| mock_server_returns_tool_use_sequence | ✅ |
| mock_server_429_then_200_shows_retry | ✅ |

### Doc Tests

- **Tests:** 0
- **Status:** No doc tests
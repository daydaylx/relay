# Tests

Unit and simulator tests live next to the code (`crates/relay/src/**`, run with `cargo test`); the
real-system test is `nix/tests/activation.nix`. See `docs/DEVELOPMENT.md` for the architecture.

Mandatory safety categories and where they are demonstrated:

| Category | Test (module) |
| --- | --- |
| source drift | `source_drift_between_plan_and_apply_aborts_before_any_mutation` (engine) |
| candidate isolation | `plan_builds_an_isolated_candidate_and_leaves_the_live_system_untouched` (engine) |
| failed evaluation | `failed_evaluation_fails_the_plan_without_touching_anything_and_keeps_stderr_private` (engine) |
| failed build | `failed_build_fails_the_plan_and_removes_the_candidate` (engine) |
| switch inhibitor | `a_switch_inhibitor_forces_the_boot_path_and_is_never_bypassed` (engine); VM: "a switch inhibitor forces the boot path" |
| failed test activation | `failed_test_activation_restores_source_and_runtime` (engine) |
| failed health check | `failed_health_check_rolls_back_because_test_is_not_a_rollback` (engine); VM: "a unit that crashes after activation…" |
| reboot-required change | `a_kernel_change_is_reboot_required_and_verified_after_the_reboot` (engine) |
| protected resource rejection | `protected_resources_are_blocked_by_code_for_every_mvp_category` (change), `protected_resources_are_rejected_before_anything_is_created` (engine) |
| source rollback | `rollback_never_overwrites_a_concurrent_foreign_edit_of_the_managed_module` (engine) |
| runtime rollback | `vlc_can_be_added_removed_and_the_last_change_undone` (engine); VM: "undo restores source and runtime together" |
| crash/restart during a change | `a_crash_during_test_activation_is_rolled_back_after_restart` and the other `a_crash_…` tests (engine) |
| AI unavailable | `intents_from_any_frontend_run_the_same_pipeline_without_any_ai_provider` (engine) |
| malformed model intent | `malformed_model_intents_are_rejected_before_rendering` (intent) |
| model proposal cross-check against the index | `invented_names_and_impossible_values_are_caught_against_the_local_index` (ai) |
| hostile or confused model output | `hostile_or_confused_model_output_never_reaches_planning` (engine), `malformed_or_dangerous_model_output_is_rejected_without_any_side_effect` (ai) |
| provider secrets | `the_openai_compatible_provider_keeps_the_key_out_of_arguments_and_parses_the_answer` (ai), `stdin_is_delivered_without_appearing_in_the_process_arguments_or_debug_output` (exec) |
| model never confirms for itself | `a_model_originated_action_can_never_be_confirmed_with_yes` (ask); VM: "the natural-language front end…" |
| desktop health gate | `a_change_that_costs_the_desktop_a_monitor_is_rolled_back` and the other desktop tests (engine) |
| compositor is read-only | `only_the_allow_listed_read_only_requests_exist` (hypr) |
| stale options index | `rejects_cache_when_any_identity_field_changes` (index) |

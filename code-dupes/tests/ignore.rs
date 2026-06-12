mod common;

dupes_cli_test_support::cli_support_tests! {
    common::code_dupes;
    ignore_workflow => dupes_cli_test_support::ignore_workflow;
    ignore_near_duplicate_workflow => dupes_cli_test_support::ignore_near_duplicate_workflow;
    cleanup_removes_stale_entries => dupes_cli_test_support::cleanup_removes_stale_entries;
    cleanup_dry_run => dupes_cli_test_support::cleanup_dry_run;
}

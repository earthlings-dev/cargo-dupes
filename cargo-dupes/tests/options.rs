mod common;

dupes_cli_test_support::cli_support_tests! {
    common::cargo_dupes;
    min_nodes_option => dupes_cli_test_support::min_nodes_option;
    min_lines_option => dupes_cli_test_support::min_lines_option;
    exclude_option => dupes_cli_test_support::exclude_option(false, "No source files");
    exclude_tests_flag_reduces_duplicates => dupes_cli_test_support::exclude_tests_flag_reduces_duplicates;
    exclude_tests_text_report => dupes_cli_test_support::exclude_tests_text_report;
    dimension_option_limits_reported_dimensions => dupes_cli_test_support::dimension_option_limits_reported_dimensions;
    error_on_nonexistent_path => dupes_cli_test_support::error_on_nonexistent_path("No source files");
    help_works => dupes_cli_test_support::help_works;
}

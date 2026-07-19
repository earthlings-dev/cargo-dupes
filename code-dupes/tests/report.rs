//! Shared report/stats output suite stamped against the `code-dupes` binary.

// jscpd:ignore-start

mod common;

dupes_cli_test_support::cli_support_tests! {
    common::code_dupes;
    report_exact_dupes_fixture => dupes_cli_test_support::report_exact_dupes_fixture;
    report_no_dupes_fixture => dupes_cli_test_support::report_no_dupes_fixture;
    report_mixed_fixture => dupes_cli_test_support::report_mixed_fixture;
    stats_shows_summary => dupes_cli_test_support::stats_shows_summary;
    stats_shows_duplicate_lines => dupes_cli_test_support::stats_shows_duplicate_lines;
    default_command_is_report => dupes_cli_test_support::default_command_is_report;
    near_dupes_detected => dupes_cli_test_support::near_dupes_detected;
    json_format_stats => dupes_cli_test_support::json_format_stats;
    json_format_report => dupes_cli_test_support::json_format_report;
    json_stats_includes_line_counts => dupes_cli_test_support::json_stats_includes_line_counts;
}

// jscpd:ignore-end

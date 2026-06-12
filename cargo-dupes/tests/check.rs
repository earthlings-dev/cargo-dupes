// jscpd:ignore-start

mod common;

dupes_cli_test_support::cli_support_tests! {
    common::cargo_dupes;
    check_no_thresholds_passes_with_duplicates => dupes_cli_test_support::check_no_thresholds_passes_with_duplicates;
    check_fails_with_duplicates => dupes_cli_test_support::check_fails_with_duplicates;
    check_passes_with_high_threshold => dupes_cli_test_support::check_passes_with_high_threshold;
    check_no_dupes_passes => dupes_cli_test_support::check_no_dupes_passes;
    check_fails_with_percentage_threshold_exceeded => dupes_cli_test_support::check_fails_with_percentage_threshold_exceeded;
    check_passes_with_generous_percentage_threshold => dupes_cli_test_support::check_passes_with_generous_percentage_threshold;
    check_absolute_passes_percentage_fails => dupes_cli_test_support::check_absolute_passes_percentage_fails;
}

// jscpd:ignore-end

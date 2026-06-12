// jscpd:ignore-start

mod common;

dupes_cli_test_support::cli_support_tests! {
    common::cargo_dupes;
    sub_function_detects_duplicate_branches => dupes_cli_test_support::sub_function_detects_duplicate_branches;
    sub_function_shows_parent_names => dupes_cli_test_support::sub_function_shows_parent_names;
    sub_function_stats_shown => dupes_cli_test_support::sub_function_stats_shown;
    sub_function_json_stats => dupes_cli_test_support::sub_function_json_stats;
    without_sub_function_flag_no_sub_sections => dupes_cli_test_support::without_sub_function_flag_no_sub_sections;
    without_sub_function_json_no_sub_fields => dupes_cli_test_support::without_sub_function_json_no_sub_fields;
    sub_function_min_sub_nodes_filters => dupes_cli_test_support::sub_function_min_sub_nodes_filters;
}

// jscpd:ignore-end

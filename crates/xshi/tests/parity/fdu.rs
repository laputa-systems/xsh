//! fdu browser scenarios adapted from ish's `fdu_pty.rs` and `fdu_snapshots.rs`.

use super::{key, run_scenario, Fixture, Live};

fn tree_fixture() -> Fixture {
    let inner = "a".repeat(3000);
    let other = "o".repeat(4);
    let deep = "b".repeat(20000);
    let top = "t".repeat(12000);
    Fixture::new()
        .dir("root")
        .dir("root/alpha")
        .dir("root/beta")
        .file("root/alpha/inner.txt", &inner)
        .file("root/alpha/other.txt", &other)
        .file("root/beta/deep.txt", &deep)
        .file("root/top.txt", &top)
}

fn exclusion_fixture() -> Fixture {
    Fixture::new()
        .dir("root")
        .dir("root/alpha")
        .dir("root/beta")
        .file("root/alpha/inner.txt", "a")
        .file("root/beta/deep.txt", "b")
        .file("root/top.txt", "t")
}

fn ready(sh: &mut Live, entries: usize) {
    sh.wait_screen_contains(&format!("{entries} entries"));
}

fn ready_by_name(sh: &mut Live, entries: usize) {
    ready(sh, entries);
    sh.text("s");
    sh.wait_screen_contains("Sorted by name");
    sh.keys(key::HOME);
}

fn quit(sh: &mut Live) {
    sh.text("q");
    sh.wait_prompt();
}

#[test]
fn startup_shows_a_minimal_title_and_the_entry_count_and_restores_the_terminal() {
    run_scenario(
        "fdu_startup_shows_a_minimal_title_and_the_entry_count",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "root (3 shown, 6 total,");
            sh.frame("ready");
            quit(sh);
        },
    );
}

#[test]
fn repeated_exclusions_hide_exact_root_relative_paths_in_the_browser() {
    run_scenario(
        "fdu_repeated_exclusions_hide_exact_root_relative_paths",
        &exclusion_fixture(),
        |sh| {
            sh.line_until(
                "fdu --interactive -x alpha --exclude beta root",
                "root (1 shown, 1 total,",
            );
            sh.frame("excluded");
            quit(sh);
        },
    );
}

#[test]
fn directory_with_excluded_contents_is_deleted_around_the_excluded_entry() {
    let fixture = Fixture::new()
        .dir("root")
        .dir("root/alpha")
        .dir("root/alpha/nested")
        .file("root/alpha/nested/hidden.txt", "excluded")
        .file("root/alpha/visible.txt", "included");
    run_scenario(
        "fdu_directory_with_excluded_contents_is_deleted_around_the_excluded_entry",
        &fixture,
        |sh| {
            sh.line_until(
                "fdu --interactive --exclude alpha/nested root",
                "2 entries",
            );
            sh.text("d");
            sh.wait_screen_contains("Marked 1 item");
            sh.keys(key::TAB);
            sh.keys(key::CTRL_R);
            sh.wait_screen_contains("Permanently delete");
            sh.keys(key::ENTER);
            sh.wait_screen_contains("Deleted 1");
            sh.frame("deleted");
            quit(sh);
            sh.effect_listing("alpha", "root/alpha");
        },
    );
}

#[test]
fn delete_confirmation_shows_root_and_entry_counts_and_leaves_unconfirmed_marks() {
    run_scenario(
        "fdu_delete_confirmation_shows_root_and_entry_counts",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "6 entries");
            ready_by_name(sh, 6);
            sh.text("d");
            sh.wait_screen_contains("Marked 1 item");
            sh.keys(key::TAB);
            sh.keys(key::CTRL_R);
            sh.wait_screen_contains("Permanently delete 1 item (3 entries,");
            sh.frame("confirm");
            sh.keys(key::ESC);
            sh.wait_until_absent("Permanently delete");
            sh.frame("cancelled");
            sh.effect_file("inner", "root/alpha/inner.txt");
            quit(sh);
        },
    );
}

#[test]
fn help_screen_opens_and_closes() {
    run_scenario(
        "fdu_help_screen_opens_and_closes",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "6 entries");
            sh.text("?");
            sh.wait_screen_contains("Tab switches");
            sh.frame("help");
            sh.keys(key::ESC);
            sh.wait_until_absent("Tab switches");
            sh.frame("closed");
            quit(sh);
        },
    );
}

#[test]
fn read_only_mode_disables_deletion() {
    run_scenario(
        "fdu_read_only_mode_disables_deletion",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive --read-only root", "6 entries");
            ready_by_name(sh, 6);
            sh.text("d");
            sh.wait_screen_contains("Marked 1 item");
            sh.keys(key::TAB);
            sh.keys(key::CTRL_R);
            sh.wait_screen_contains("This session is read only; deletion is disabled.");
            sh.frame("read_only");
            sh.effect_file("inner", "root/alpha/inner.txt");
            quit(sh);
        },
    );
}

#[test]
fn minus_toggles_between_side_by_side_and_stacked_panes() {
    run_scenario(
        "fdu_minus_toggles_between_side_by_side_and_stacked_panes",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "6 entries");
            ready_by_name(sh, 6);
            sh.text("d");
            sh.wait_screen_contains("Marked 1 item");
            sh.frame("side_by_side");
            sh.text("-");
            sh.wait_screen("stacked panes", |screen| {
                let row = |needle: &str| {
                    (0..screen.row_count()).find(|row| {
                        screen.row(*row).is_some_and(|text| text.contains(needle))
                    })
                };
                matches!((row("root ("), row("Marked 1 item")), (Some(list), Some(marked)) if list < marked)
            });
            sh.frame("stacked");
            sh.text("-");
            sh.wait_screen("side by side again", |screen| {
                let row = |needle: &str| {
                    (0..screen.row_count()).find(|row| {
                        screen.row(*row).is_some_and(|text| text.contains(needle))
                    })
                };
                row("root (") == row("Marked 1 item")
            });
            sh.frame("side_by_side_again");
            quit(sh);
        },
    );
}

#[test]
fn s_cycles_between_size_name_and_date_order() {
    run_scenario(
        "fdu_s_cycles_between_size_name_and_date_order",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "6 entries");
            sh.frame("size_order");
            sh.text("s");
            sh.wait_screen_contains("Sorted by name");
            sh.frame("name_order");
            sh.text("s");
            sh.wait_screen_contains("Sorted by date");
            sh.frame("date_order");
            sh.text("s");
            sh.wait_screen_contains("Sorted by size");
            sh.frame("size_order_again");
            quit(sh);
        },
    );
}

#[test]
fn unicode_filter_input_and_backspace_preserve_complete_characters() {
    let fixture = Fixture::new()
        .dir("root")
        .file("root/日本-é.txt", "x")
        .file("root/other.txt", "x");
    run_scenario(
        "fdu_unicode_filter_input_and_backspace",
        &fixture,
        |sh| {
            sh.line_until("fdu --interactive root", "2 entries");
            sh.text("/");
            sh.text("日本é");
            sh.wait_screen_contains("/ 日本é");
            sh.keys(key::BACKSPACE);
            sh.wait_screen_contains("/ 日本");
            sh.keys(key::ENTER);
            sh.wait_screen_contains("(1 shown, 2 total");
            sh.frame("filtered");
            quit(sh);
        },
    );
}

#[test]
fn profile_large_layout_fixture() {
    run_scenario(
        "fdu_profile_large_layout_fixture",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "6 entries");
            sh.frame("profile_ready");
            quit(sh);
        },
    );
}

#[test]
fn rapid_input_handoff_burst_does_not_drop_queued_quit() {
    run_scenario(
        "fdu_rapid_input_handoff_burst",
        &tree_fixture(),
        |sh| {
            sh.keys(b"fdu --interactive root\rq");
            sh.wait_prompt();
            sh.frame("after_burst");
            sh.keys(b"q\r");
            sh.wait_prompt();
            sh.frame("after_quit");
        },
    );
}

#[test]
fn quit_preserves_the_remaining_command_in_the_same_input_burst() {
    run_scenario(
        "fdu_quit_preserves_command_typeahead",
        &tree_fixture(),
        |sh| {
            sh.keys(b"fdu --interactive root\rqecho handoff-survived >handoff-marker\r");
            sh.wait_prompt();
            sh.line("cat handoff-marker");
            sh.effect_file("queued_command", "handoff-marker");
        },
    );
}

#[test]
fn sigterm_terminates_shell_running_fdu() {
    run_scenario(
        "fdu_sigterm_terminates_shell_running_fdu",
        &tree_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "6 entries");
            sh.signal(libc::SIGTERM);
            sh.expect_exit("sigterm");
        },
    );
}

fn snapshot_fixture() -> Fixture {
    let mut fixture = tree_fixture();
    fixture = fixture.file("root/alpha/extra.txt", &"e".repeat(900));
    fixture
}

#[test]
fn snapshot_ready_listing() {
    run_scenario(
        "fdu_snapshot_ready_listing",
        &snapshot_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            sh.frame("ready_100x24");
            quit(sh);
        },
    );
}

#[test]
fn snapshot_marked_pane_beside_the_list() {
    run_scenario(
        "fdu_snapshot_marked_pane_beside_the_list",
        &snapshot_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            ready_by_name(sh, 7);
            sh.text("d");
            sh.text("d");
            sh.wait_screen_contains("Marked 2 items");
            sh.frame("marked_100x24");
            sh.keys(key::TAB);
            sh.frame("marked_focus_100x24");
            quit(sh);
        },
    );
}

#[test]
fn snapshot_stacked_panes() {
    run_scenario(
        "fdu_snapshot_stacked_panes",
        &snapshot_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            ready_by_name(sh, 7);
            sh.text("d");
            sh.wait_screen_contains("Marked 1 item");
            sh.text("-");
            sh.wait_screen("stacked panes", |screen| {
                let row = |needle: &str| {
                    (0..screen.row_count()).find(|row| {
                        screen.row(*row).is_some_and(|text| text.contains(needle))
                    })
                };
                matches!((row("root ("), row("Marked 1 item")), (Some(list), Some(marked)) if list < marked)
            });
            sh.frame("stacked_100x24");
            quit(sh);
        },
    );
}

#[test]
fn snapshot_narrow_terminal_shows_one_pane() {
    run_scenario(
        "fdu_snapshot_narrow_terminal_shows_one_pane",
        &snapshot_fixture().size(24, 50),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            ready_by_name(sh, 7);
            sh.text("d");
            sh.text("d");
            sh.keys(key::TAB);
            sh.wait_screen_contains("Marked 2 items");
            sh.frame("narrow_marked_50x24");
            quit(sh);
        },
    );
}

#[test]
fn snapshot_inside_a_marked_directory() {
    run_scenario(
        "fdu_snapshot_inside_a_marked_directory",
        &snapshot_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            ready_by_name(sh, 7);
            sh.text("d");
            sh.keys(key::HOME);
            sh.keys(key::ENTER);
            sh.wait_screen_contains("Inside marked directory");
            sh.frame("covered_100x24");
            quit(sh);
        },
    );
}

#[test]
fn snapshot_help_overlay() {
    run_scenario(
        "fdu_snapshot_help_overlay",
        &snapshot_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            sh.text("?");
            sh.wait_screen_contains("closes help");
            sh.frame("help_100x24");
            sh.keys(key::ESC);
            sh.wait_until_absent("closes help");
            quit(sh);
        },
    );
}

#[test]
fn snapshot_filter_prompt_and_applied_filter() {
    run_scenario(
        "fdu_snapshot_filter_prompt_and_applied_filter",
        &snapshot_fixture(),
        |sh| {
            sh.line_until("fdu --interactive root", "7 entries");
            sh.text("/");
            sh.text("alpha");
            sh.wait_screen_contains("/ alpha");
            sh.frame("filter_prompt_100x24");
            sh.keys(key::ENTER);
            sh.wait_screen_contains("filter \"alpha\"");
            sh.frame("filtered_100x24");
            quit(sh);
        },
    );
}

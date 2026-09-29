//! Scenarios beyond ish's own PTY suite. They are recorded from ish all the
//! same: they push the contract ish documents (editing keys, Unicode,
//! completion, history ranking) through inputs its tests never send.

use super::scenarios::parity;
use super::{Fixture, Live, key, paste, run_scenario};

parity!(
    edit_home_end_and_ctrl_a_e,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("hello world");
        sh.keys(key::HOME);
        sh.text("X");
        sh.frame("home");
        sh.keys(key::END);
        sh.text("Y");
        sh.frame("end");
        sh.keys(key::CTRL_A);
        sh.text("A");
        sh.keys(key::CTRL_E);
        sh.text("E");
        sh.frame("ctrl_a_e");
    }
);

parity!(
    edit_delete_and_ctrl_d_delete_forward,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("abcdef");
        sh.repeat(key::LEFT, 3);
        sh.keys(key::DELETE);
        sh.frame("after_delete");
        sh.keys(key::CTRL_D);
        sh.frame("after_ctrl_d");
        sh.keys(key::END);
        sh.keys(key::DELETE);
        sh.frame("delete_at_end_is_a_no_op");
    }
);

parity!(edit_word_motions, Fixture::new(), |sh: &mut Live| {
    sh.text("one two  three-four five");
    sh.repeat(key::CTRL_LEFT, 2);
    sh.text("X");
    sh.frame("ctrl_left_twice");
    sh.keys(key::ALT_B);
    sh.text("Y");
    sh.frame("alt_b");
    sh.keys(key::CTRL_RIGHT);
    sh.text("Z");
    sh.frame("ctrl_right");
    sh.keys(key::ALT_F);
    sh.text("W");
    sh.frame("alt_f");
    sh.repeat(key::CTRL_RIGHT, 5);
    sh.text("!");
    sh.frame("ctrl_right_stops_at_end");
    sh.repeat(key::CTRL_LEFT, 9);
    sh.text("^");
    sh.frame("ctrl_left_stops_at_start");
});

parity!(edit_word_kills, Fixture::new(), |sh: &mut Live| {
    sh.text("alpha beta gamma");
    sh.keys(key::CTRL_W);
    sh.frame("ctrl_w");
    sh.keys(key::CTRL_BACKSPACE);
    sh.frame("ctrl_backspace");
    sh.text("delta epsilon zeta");
    sh.repeat(key::ALT_B, 2);
    sh.keys(key::ALT_D);
    sh.frame("alt_d");
    sh.keys(key::CTRL_Y);
    sh.frame("yank");
    sh.repeat(key::CTRL_W, 6);
    sh.frame("kill_past_start");
});

parity!(
    edit_kill_ring_is_shared_between_kills,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("one two three");
        sh.repeat(key::LEFT, 6);
        sh.keys(key::CTRL_K);
        sh.frame("kill_to_end");
        sh.keys(key::CTRL_U);
        sh.frame("kill_to_start");
        sh.keys(key::CTRL_Y);
        sh.frame("yank_last_kill");
        sh.keys(key::CTRL_Y);
        sh.frame("yank_twice");
        sh.keys(key::CTRL_U);
        sh.keys(key::CTRL_W);
        sh.keys(key::CTRL_Y);
        sh.frame("yank_after_empty_kills");
    }
);

parity!(
    edit_utf8_cursor_and_backspace,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("héllo wörld");
        sh.frame("typed");
        sh.repeat(key::LEFT, 3);
        sh.text("é");
        sh.frame("inserted_before_ö");
        sh.repeat(key::BACKSPACE, 2);
        sh.frame("backspaced");
        sh.keys(key::HOME);
        sh.keys(key::DELETE);
        sh.frame("deleted_h");
        sh.keys(key::END);
        sh.keys_to_prompt(key::ENTER);
        sh.frame("submitted");
    }
);

parity!(edit_wide_characters, Fixture::new(), |sh: &mut Live| {
    sh.text("日本語text");
    sh.frame("typed");
    sh.repeat(key::LEFT, 5);
    sh.frame("cursor_after_wide_run");
    sh.keys(key::BACKSPACE);
    sh.frame("backspace_removes_one_wide_character");
    sh.text("字");
    sh.frame("insert_wide");
    sh.keys(key::CTRL_K);
    sh.frame("kill_wide_tail");
});

parity!(
    edit_combining_marks_and_emoji,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("cafe\u{301} 👍🏽 ok");
        sh.frame("typed");
        sh.repeat(key::LEFT, 3);
        sh.frame("cursor_over_emoji");
        sh.keys(key::BACKSPACE);
        sh.frame("backspace_over_modifier");
        sh.keys(key::END);
        sh.repeat(key::BACKSPACE, 4);
        sh.frame("backspace_into_combining_mark");
    }
);

parity!(
    edit_wide_characters_wrap_at_the_edge,
    Fixture::new().size_over_prompt(12, 9),
    |sh: &mut Live| {
        sh.text("日本語日本語日本語日本語");
        sh.frame("wrapped");
        sh.repeat(key::UP, 1);
        sh.frame("up_one_row");
        sh.keys(key::DOWN);
        sh.frame("down_one_row");
        sh.keys(key::HOME);
        sh.frame("home");
    }
);

parity!(
    edit_enter_mid_line_submits_the_whole_line,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("echo abcdef");
        sh.repeat(key::LEFT, 3);
        sh.keys_to_prompt(key::ENTER);
        sh.frame("submitted");
    }
);

parity!(
    edit_ctrl_l_keeps_the_pending_line,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("echo before");
        sh.text("echo partial");
        sh.repeat(key::LEFT, 3);
        sh.keys(key::CTRL_L);
        sh.frame("cleared");
        sh.text("X");
        sh.frame("cursor_kept");
    }
);

parity!(
    edit_ctrl_c_discards_the_pending_line,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("echo never");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("cancelled");
        sh.keys(key::UP);
        sh.frame("cancelled_line_is_not_history");
    }
);

parity!(
    edit_multiline_paste_navigates_between_lines,
    Fixture::new(),
    |sh: &mut Live| {
        sh.keys(&paste("echo 'one\ntwo\nthree'"));
        sh.frame("pasted");
        sh.keys(key::UP);
        sh.frame("up");
        sh.keys(key::UP);
        sh.frame("up_again");
        sh.text("X");
        sh.frame("edit_middle_line");
        sh.keys(key::DOWN);
        sh.keys(key::DOWN);
        sh.frame("down_to_last");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("submitted");
    }
);

parity!(
    edit_multiline_up_at_first_row_recalls_history,
    Fixture::new().history(&["echo older", "echo newer"]),
    |sh: &mut Live| {
        sh.keys(&paste("echo 'a\nb'"));
        sh.keys(key::UP);
        sh.keys(key::UP);
        sh.frame("first_row");
        sh.keys(key::UP);
        sh.frame("history_at_boundary");
        sh.keys(key::DOWN);
        sh.frame("back_down");
    }
);

parity!(
    history_prefix_search_with_up_and_down,
    Fixture::new().history(&[
        "git status",
        "echo unrelated",
        "git commit -m one",
        "ls",
        "git push origin main",
    ]),
    |sh: &mut Live| {
        sh.text("git");
        sh.keys(key::UP);
        sh.frame("newest_git");
        sh.keys(key::UP);
        sh.frame("older_git");
        sh.keys(key::UP);
        sh.frame("oldest_git");
        sh.keys(key::UP);
        sh.frame("no_more_matches");
        sh.keys(key::DOWN);
        sh.frame("back_one");
        sh.repeat(key::DOWN, 3);
        sh.frame("back_to_typed_prefix");
    }
);

parity!(
    history_up_down_plain_walk_restores_the_draft,
    Fixture::new().history(&["echo one", "echo two", "echo three"]),
    |sh: &mut Live| {
        sh.text("draft");
        sh.repeat(key::UP, 1);
        sh.frame("prefix_no_match_keeps_draft");
        sh.keys(key::CTRL_U);
        sh.repeat(key::UP, 3);
        sh.frame("oldest");
        sh.repeat(key::UP, 2);
        sh.frame("stays_at_oldest");
        sh.repeat(key::DOWN, 4);
        sh.frame("past_newest_is_empty");
    }
);

parity!(
    autosuggest_shows_dim_suffix_and_accepts_it,
    Fixture::new().history(&["echo autosuggest here", "ls -la"]),
    |sh: &mut Live| {
        sh.text("echo au");
        sh.frame("suggested");
        sh.keys(key::RIGHT);
        sh.frame("accepted_with_right");
        sh.keys(key::CTRL_U);
        sh.text("echo aut");
        sh.keys(key::END);
        sh.frame("accepted_with_end");
        sh.keys(key::CTRL_U);
        sh.text("echo aut");
        sh.keys(key::CTRL_E);
        sh.frame("accepted_with_ctrl_e");
        sh.keys(key::CTRL_U);
        sh.text("zzz");
        sh.frame("no_suggestion");
        sh.keys(key::CTRL_U);
        sh.text("echo aut");
        sh.repeat(key::LEFT, 1);
        sh.frame("no_suggestion_when_cursor_moved_back");
    }
);

parity!(
    autosuggest_follows_the_most_recent_match,
    Fixture::new().history(&["echo one apple", "echo two apple", "echo one banana"]),
    |sh: &mut Live| {
        sh.text("echo one");
        sh.frame("newest_match");
        sh.text(" a");
        sh.frame("narrowed");
        sh.keys(key::BACKSPACE);
        sh.frame("widened_again");
    }
);

parity!(
    completion_on_an_empty_line_inserts_cd,
    Fixture::new(),
    |sh: &mut Live| {
        sh.keys(key::TAB);
        sh.frame("cd_inserted");
        sh.keys(key::TAB);
        sh.frame("second_tab_lists_directories");
    }
);

parity!(
    completion_after_cd_offers_only_directories,
    Fixture::new()
        .dir("alpha")
        .dir("beta")
        .file("afile.txt", "")
        .symlink("alink", "alpha")
        .dir(".hidden-dir"),
    |sh: &mut Live| {
        sh.text("cd ");
        sh.keys(key::TAB);
        sh.frame("directories_only");
        sh.keys(key::ESC);
        sh.text("a");
        sh.keys(key::TAB);
        sh.frame("prefix_a");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_U);
        sh.text("cd .");
        sh.keys(key::TAB);
        sh.frame("dot_shows_hidden");
    }
);

parity!(
    completion_colors_directories_symlinks_and_executables,
    Fixture::new()
        .dir("adir")
        .symlink("alink", "adir")
        .executable("aexec.sh", "#!/bin/sh\n")
        .file("aplain.txt", ""),
    |sh: &mut Live| {
        sh.text("ls a");
        sh.keys(key::TAB);
        sh.frame("grid");
        sh.keys(key::TAB);
        sh.frame("first_selected");
        sh.keys(key::TAB);
        sh.frame("second_selected");
    }
);

parity!(
    completion_orders_paths_by_recent_modification,
    Fixture::new()
        .file_at("f-oldest.txt", "", 1_600_000_000)
        .file_at("f-middle.txt", "", 1_650_000_000)
        .file_at("f-newest.txt", "", 1_690_000_000)
        .dir_at("f-newest-dir", 1_695_000_000),
    |sh: &mut Live| {
        sh.text("cat f");
        sh.keys(key::TAB);
        sh.frame("newest_first");
    }
);

parity!(
    completion_grid_scrolls_past_ten_rows,
    {
        let mut fixture = Fixture::new().size(24, 60);
        for index in 0..90 {
            fixture = fixture.file_at(&format!("file-{index:02}.dat"), "", 1_600_000_000 + index);
        }
        fixture
    },
    |sh: &mut Live| {
        sh.text("cat file");
        sh.keys(key::TAB);
        sh.frame("first_page");
        sh.repeat(key::DOWN, 12);
        sh.frame("scrolled_down");
        sh.repeat(key::RIGHT, 3);
        sh.frame("moved_right");
        sh.repeat(key::UP, 15);
        sh.frame("scrolled_back_up");
    }
);

parity!(
    completion_typing_filters_and_backspace_widens,
    Fixture::new()
        .file("file-a1", "")
        .file("file-a2", "")
        .file("file-b1", "")
        .file("other", ""),
    |sh: &mut Live| {
        sh.text("cat f");
        sh.keys(key::TAB);
        sh.frame("all_files");
        sh.text("ile-a");
        sh.frame("filtered_to_a");
        sh.keys(key::BACKSPACE);
        sh.frame("widened");
        sh.text("b");
        sh.frame("single_candidate");
    }
);

parity!(
    completion_arrows_navigate_and_enter_accepts_without_submitting,
    Fixture::new()
        .file("a-one", "")
        .file("a-two", "")
        .file("a-three", "")
        .file("a-four", ""),
    |sh: &mut Live| {
        sh.text("cat a");
        sh.keys(key::TAB);
        sh.keys(key::DOWN);
        sh.frame("down");
        sh.keys(key::RIGHT);
        sh.frame("right");
        sh.keys(key::UP);
        sh.frame("up");
        sh.keys(key::LEFT);
        sh.frame("left");
        sh.keys(key::ENTER);
        sh.frame("accepted_not_submitted");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("submitted");
    }
);

parity!(
    completion_extends_to_the_common_prefix,
    Fixture::new()
        .file("src/main.rs", "")
        .file("src/mod.rs", "")
        .file("src/module_two.rs", ""),
    |sh: &mut Live| {
        sh.text("cat src/m");
        sh.keys(key::TAB);
        sh.frame("grid_for_m");
        sh.keys(key::ESC);
        sh.text("od");
        sh.keys(key::TAB);
        sh.frame("after_od");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_U);
        sh.text("cat src/ma");
        sh.keys(key::TAB);
        sh.frame("unique");
    }
);

parity!(
    completion_of_tilde_and_hidden_entries,
    Fixture::new()
        .file(".profile-like", "")
        .file("visible", "")
        .dir(".config-like"),
    |sh: &mut Live| {
        sh.text("ls ~/");
        sh.keys(key::TAB);
        sh.frame("home_without_hidden");
        sh.keys(key::ESC);
        sh.text(".");
        sh.keys(key::TAB);
        sh.frame("home_hidden");
    }
);

parity!(
    completion_escapes_spaces_and_quotes_in_names,
    Fixture::new()
        .file("my file.txt", "")
        .file("it's here", "")
        .file("plain", ""),
    |sh: &mut Live| {
        sh.text("cat my");
        sh.keys(key::TAB);
        sh.frame("space_escaped");
        sh.keys(key::CTRL_U);
        sh.text("cat it");
        sh.keys(key::TAB);
        sh.frame("quote_escaped");
    }
);

parity!(
    completion_in_command_position_uses_path_and_builtins,
    Fixture::new()
        .executable("bin/zebra", "#!/bin/sh\n")
        .executable("bin/zeta", "#!/bin/sh\n")
        .file("bin/zero-not-executable", "")
        .executable("elsewhere/zulu", "#!/bin/sh\n"),
    |sh: &mut Live| {
        sh.line("set PATH $HOME/bin");
        sh.text("ze");
        sh.keys(key::TAB);
        sh.frame("path_commands");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_U);
        sh.text("hist");
        sh.keys(key::TAB);
        sh.frame("builtin_completes");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_U);
        sh.text("z");
        sh.keys(key::TAB);
        sh.frame("z_prefix");
    }
);

parity!(
    completion_after_pipe_and_redirect_operators,
    Fixture::new()
        .executable("bin/upcase", "#!/bin/sh\n")
        .file("out.txt", ""),
    |sh: &mut Live| {
        sh.line("set PATH $HOME/bin");
        sh.text("echo hi | upc");
        sh.keys(key::TAB);
        sh.frame("command_after_pipe");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_U);
        sh.text("echo hi > ou");
        sh.keys(key::TAB);
        sh.frame("file_after_redirect");
    }
);

parity!(
    completion_of_ssh_hosts_from_config_and_known_hosts,
    Fixture::new()
        .file(".ssh/config", "Host alpha-box beta-box\n  HostName 10.0.0.1\nHost *.wild\nHost gamma\n")
        .file(".ssh/known_hosts", "alpha-known,192.0.2.1 ssh-ed25519 AAAA\ndelta.example.com ssh-rsa BBBB\n|1|hashed ssh-rsa CCCC\n"),
    |sh: &mut Live| {
        sh.text("ssh ");
        sh.keys(key::TAB);
        sh.frame("all_hosts");
        sh.keys(key::ESC);
        sh.text("al");
        sh.keys(key::TAB);
        sh.frame("prefix_al");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_U);
        sh.text("scp file user@g");
        sh.keys(key::TAB);
        sh.frame("user_at_host");
    }
);

parity!(
    completion_with_escape_ctrl_c_and_ctrl_l_inside_the_grid,
    Fixture::new().file("aa", "").file("ab", "").file("ac", ""),
    |sh: &mut Live| {
        sh.text("cat a");
        sh.keys(key::TAB);
        sh.keys(key::TAB);
        sh.frame("selected");
        sh.keys(key::CTRL_L);
        sh.frame("after_ctrl_l");
        sh.keys(key::CTRL_C);
        sh.frame("after_ctrl_c");
    }
);

parity!(
    alias_expands_on_space_only_in_command_position,
    Fixture::new().config("alias ll l\nalias gs git status\n"),
    |sh: &mut Live| {
        sh.text("ll ");
        sh.frame("expanded");
        sh.keys(key::CTRL_U);
        sh.text("echo ll ");
        sh.frame("argument_not_expanded");
        sh.keys(key::CTRL_U);
        sh.text("true; gs ");
        sh.frame("after_semicolon");
        sh.keys(key::CTRL_U);
        sh.text("echo hi | ll ");
        sh.frame("after_pipe");
        sh.keys(key::CTRL_U);
        sh.text("'ll' ");
        sh.frame("quoted_not_expanded");
    }
);

parity!(
    history_search_is_case_insensitive_subsequence,
    Fixture::new().history(&["git status", "GIT DIFF", "grep -rn status src", "echo gst"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("GST");
        sh.frame("uppercase_query");
        sh.keys(key::BACKSPACE);
        sh.frame("shorter_query");
        sh.repeat(key::BACKSPACE, 2);
        sh.frame("empty_query_lists_recent_first");
    }
);

parity!(
    history_search_prefers_current_directory_and_ancestors,
    Fixture::new()
        .dir("project/src")
        .dir("other")
        .history_records(&[
            ("project", "make build project"),
            ("other", "make build other"),
            ("", "make build home"),
            ("project/src", "make build src"),
            ("other", "make build other again"),
        ])
        .cwd("project/src"),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("make build");
        sh.frame("boosted_by_directory");
    }
);

parity!(
    history_search_ranks_contiguous_and_boundary_matches,
    Fixture::new().history(&[
        "ls target/debug/",
        "cd d-e-b-u-g",
        "echo the best test",
        "cargo test --release",
        "grep deb file",
        "echo debug output",
        "docker exec -it bash",
    ]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("deb");
        sh.frame("deb");
        sh.keys(key::CTRL_U);
        sh.text("test");
        sh.frame("test");
        sh.keys(key::CTRL_U);
        sh.text("target");
        sh.frame("target");
    }
);

parity!(
    history_search_keeps_the_edited_line_on_cancel_and_replaces_it_on_accept,
    Fixture::new().history(&["echo remembered one", "echo remembered two"]),
    |sh: &mut Live| {
        sh.text("echo draft");
        sh.keys(key::CTRL_R);
        sh.text("one");
        sh.frame("searching");
        sh.keys(key::ESC);
        sh.frame("cancelled_keeps_draft");
        sh.keys(key::CTRL_R);
        sh.text("two");
        sh.keys(key::ENTER);
        sh.frame("accepted_replaces_draft");
    }
);

parity!(
    history_records_are_deduplicated_and_trimmed,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("echo same");
        sh.line("echo same");
        sh.line("   echo padded   ");
        sh.line("echo same");
        sh.keys(key::UP);
        sh.frame("newest");
        sh.keys(key::UP);
        sh.frame("padded_is_trimmed");
        sh.keys(key::UP);
        sh.frame("no_duplicate_of_same");
        sh.effect_history("log");
    }
);

parity!(
    layout_dump_ctrl_p_leaves_the_screen_alone,
    Fixture::new()
        .file("aa", "")
        .file("ab", "")
        .history(&["echo one", "echo two"]),
    |sh: &mut Live| {
        sh.text("echo hello world");
        sh.frame("before");
        sh.keys_silent(key::CTRL_P);
        sh.frame("after_prompt_dump");
        sh.keys(key::CTRL_U);
        sh.text("cat a");
        sh.keys(key::TAB);
        sh.frame("completion_open");
        sh.keys_silent(key::CTRL_P);
        sh.frame("after_completion_dump");
        sh.keys(key::ESC);
        sh.keys(key::CTRL_R);
        sh.keys_silent(key::CTRL_P);
        sh.frame("after_history_dump");
        sh.keys(key::ESC);
        sh.effect_dumps("dumps");
    }
);

parity!(
    layout_dump_builtin_reports_where_it_wrote,
    Fixture::new().size(24, 200),
    |sh: &mut Live| {
        sh.text("echo shown");
        sh.keys_to_prompt(key::ENTER);
        let command = format!("{}-dump", sh.kind().label());
        sh.line(&command);
        sh.frame("after_dump_command");
        sh.effect_dump_count("dump_count");
    }
);

parity!(
    no_config_flag_skips_the_config_file,
    Fixture::new()
        .config("alias ll l\nset FROM_CONFIG configured\n")
        .arg("--no-config"),
    |sh: &mut Live| {
        sh.line("ll");
        sh.frame("alias_not_defined");
        sh.line("echo [$FROM_CONFIG]");
        sh.frame("variable_not_set");
    }
);

parity!(
    redirection_order_is_left_to_right,
    Fixture::new(),
    |sh: &mut Live| {
        sh.line("sh -c 'echo out; echo err >&2' 2>&1 > first.txt | cat");
        sh.line("cat first.txt");
        sh.frame("stderr_duplicated_first");
        sh.line("sh -c 'echo out; echo err >&2' > second.txt 2>&1 | cat");
        sh.line("cat second.txt");
        sh.frame("stdout_redirected_first");
        sh.line("echo [$(sh -c 'echo out; echo err >&2' 2>&1)]");
        sh.frame("substitution_captures_stderr");
    }
);

parity!(
    history_search_ctrl_n_and_ctrl_p_keys,
    Fixture::new().history(&["echo first", "echo second", "echo third"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.frame("newest_selected");
        sh.keys_silent(key::CTRL_N);
        sh.frame("after_ctrl_n");
        sh.keys_silent(key::CTRL_N);
        sh.keys_silent(key::CTRL_N);
        sh.frame("after_more_ctrl_n");
        sh.keys_silent(key::CTRL_P);
        sh.frame("after_ctrl_p");
        sh.keys(key::ENTER);
        sh.frame("accepted");
    }
);

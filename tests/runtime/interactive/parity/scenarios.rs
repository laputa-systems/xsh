//! Scenarios adapted from ish's `tests/pty.rs`. Each one is replayed against
//! the recorded ish golden; see the parent module for the recording workflow.
//! Scenario names match the ish test they adapt (see `plan-ish-parity.md`).

use super::{Fixture, Live, key, paste, run_scenario};

macro_rules! parity {
    ($name:ident, $fixture:expr, $script:expr) => {
        #[test]
        fn $name() {
            let fixture: Fixture = $fixture;
            run_scenario(stringify!($name), &fixture, $script);
        }
    };
}
pub(super) use parity;

parity!(prompt_appears_on_startup, Fixture::new(), |sh: &mut Live| {
    sh.frame("startup");
    sh.enter();
    sh.wait_prompt();
    sh.frame("after_enter");
});

parity!(echo_command, Fixture::new(), |sh: &mut Live| {
    sh.line("echo hello world");
    sh.frame("after_echo");
});

parity!(
    prompt_does_not_share_a_line_with_unterminated_external_output,
    Fixture::new().executable("bin/noeol", "#!/bin/sh\nprintf no-newline\n"),
    |sh: &mut Live| {
        sh.line("./bin/noeol");
        sh.frame("after_unterminated_output");
    }
);

parity!(
    broken_interpreter_reports_bad_interpreter,
    Fixture::new().executable("bin/badscript", "#!/nonexistent/interp\n"),
    |sh: &mut Live| {
        sh.line("./bin/badscript");
        sh.frame("after_bad_interpreter");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(pwd_builtin, Fixture::new(), |sh: &mut Live| {
    sh.line("pwd");
    sh.frame("after_pwd");
});

parity!(cd_and_pwd, Fixture::new(), |sh: &mut Live| {
    sh.line("cd / && pwd");
    sh.frame("after_cd");
});

parity!(exit_with_ctrl_d, Fixture::new(), |sh: &mut Live| {
    sh.frame("before_ctrl_d");
    sh.keys(key::CTRL_D);
    sh.expect_exit("exit_status");
});

parity!(exit_command, Fixture::new(), |sh: &mut Live| {
    sh.text("exit");
    sh.enter();
    sh.expect_exit("exit_status");
});

parity!(ctrl_c_cancels_input, Fixture::new(), |sh: &mut Live| {
    sh.text("some partial input");
    sh.frame("typed");
    sh.keys_to_prompt(key::CTRL_C);
    sh.frame("after_ctrl_c");
});

parity!(line_editing_backspace, Fixture::new(), |sh: &mut Live| {
    sh.text("echo helloo");
    sh.keys(key::BACKSPACE);
    sh.frame("edited");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(
    line_editing_up_down_navigate_wrapped_input,
    Fixture::new().size_over_prompt(24, 8),
    |sh: &mut Live| {
        sh.text("echo 012345678901234567890123456789");
        sh.frame("typed");
        sh.keys(key::UP);
        sh.frame("after_up");
        sh.keys(key::DOWN);
        sh.frame("after_down");
    }
);

parity!(line_editing_ctrl_u, Fixture::new(), |sh: &mut Live| {
    sh.text("this will be killed");
    sh.keys(key::CTRL_U);
    sh.frame("after_kill");
    sh.text("echo survived");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(line_editing_ctrl_w, Fixture::new(), |sh: &mut Live| {
    sh.text("echo remove_me keep");
    sh.repeat(key::LEFT, 5);
    sh.keys(key::CTRL_W);
    sh.frame("after_kill_word");
    sh.keys(key::CTRL_E);
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(line_editing_ctrl_delete, Fixture::new(), |sh: &mut Live| {
    sh.text("echo alpha beta");
    sh.keys(key::CTRL_DELETE);
    sh.frame("after_ctrl_delete");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(line_editing_ctrl_k_and_ctrl_y, Fixture::new(), |sh: &mut Live| {
    sh.text("echo hello world");
    sh.keys(key::CTRL_A);
    sh.repeat(key::RIGHT, 5);
    sh.keys(key::CTRL_K);
    sh.frame("after_kill");
    sh.text("yanked: ");
    sh.keys(key::CTRL_Y);
    sh.frame("after_yank");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(pipeline, Fixture::new(), |sh: &mut Live| {
    sh.line("echo 'abc def ghi' | tr ' ' '\\n' | grep -c .");
    sh.frame("after_pipeline");
});

parity!(and_or_list, Fixture::new(), |sh: &mut Live| {
    sh.line("true && echo yes");
    sh.frame("after_and");
});

parity!(or_list, Fixture::new(), |sh: &mut Live| {
    sh.line("false || echo fallback");
    sh.frame("after_or");
});

parity!(redirect_output, Fixture::new(), |sh: &mut Live| {
    sh.line("/bin/echo file_content > out.txt");
    sh.line("cat out.txt");
    sh.frame("after_cat");
    sh.effect_file("out.txt", "out.txt");
});

parity!(
    l_lists_files,
    Fixture::new()
        .file("file_a.txt", "aaa")
        .file("file_b.txt", "bbb")
        .file("subdir/.keep", ""),
    |sh: &mut Live| {
        sh.line("l");
        sh.frame("after_l");
    }
);

parity!(set_and_echo_var, Fixture::new(), |sh: &mut Live| {
    sh.line("export MY_VAR=hello_world");
    sh.line("echo $MY_VAR");
    sh.frame("after_echo");
});

parity!(exported_var_reaches_external_commands, Fixture::new(), |sh: &mut Live| {
    sh.line("export MY_VAR=hello_world");
    sh.line("env | grep MY_VAR");
    sh.frame("after_env");
});

parity!(set_var_reaches_external_commands, Fixture::new(), |sh: &mut Live| {
    sh.line("set TEST_VAR hello_world");
    sh.line("env | grep TEST_VAR");
    sh.frame("after_env");
});

parity!(set_var_joins_multiple_value_words, Fixture::new(), |sh: &mut Live| {
    sh.line("set GREETING hello world");
    sh.line("echo $GREETING");
    sh.frame("after_echo");
});

parity!(set_no_args_lists_env_vars, Fixture::new(), |sh: &mut Live| {
    sh.line("set | grep '^PATH='");
    sh.frame("after_set");
});

parity!(set_option_forms_fall_through_to_epsh, Fixture::new(), |sh: &mut Live| {
    sh.line("set -e");
    sh.frame("after_set_e");
    sh.line("echo $?");
    sh.frame("status");
});

parity!(unset_removes_var_from_children, Fixture::new(), |sh: &mut Live| {
    sh.line("export TMP_UNSET_VAR=present");
    sh.line("env | grep TMP_UNSET_VAR");
    sh.frame("before_unset");
    sh.line("unset TMP_UNSET_VAR");
    sh.line("env | grep TMP_UNSET_VAR");
    sh.frame("after_unset");
});

parity!(unset_removes_os_env_var_set_by_set, Fixture::new(), |sh: &mut Live| {
    sh.line("set TMP_OS_VAR some_value");
    sh.line("env | grep TMP_OS_VAR");
    sh.frame("before_unset");
    sh.line("unset TMP_OS_VAR");
    sh.line("env | grep TMP_OS_VAR");
    sh.frame("after_unset");
});

parity!(unset_in_same_line_not_inherited_by_children, Fixture::new(), |sh: &mut Live| {
    sh.line("export TMP_SAME_LINE=present");
    sh.line("unset TMP_SAME_LINE; env | grep TMP_SAME_LINE");
    sh.frame("after_unset_env");
});

parity!(
    unset_removes_ambient_environment_from_children,
    Fixture::new()
        .env("TMP_AMBIENT_VAR", "ambient")
        .env("1ISH_AMBIENT", "invalid-name"),
    |sh: &mut Live| {
        sh.line("env | grep AMBIENT");
        sh.frame("before_unset");
        sh.line("unset TMP_AMBIENT_VAR; env | grep AMBIENT");
        sh.frame("after_unset");
    }
);

parity!(
    unset_removes_ambient_environment_in_pipeline_children,
    Fixture::new().env("TMP_PIPE_AMBIENT", "ambient"),
    |sh: &mut Live| {
        sh.line("unset TMP_PIPE_AMBIENT; env | grep TMP_PIPE");
        sh.frame("after_unset");
    }
);

parity!(compound_list_export_reaches_child, Fixture::new(), |sh: &mut Live| {
    sh.line("cd / && export TMP_COMPOUND=value; env | grep TMP_COMPOUND");
    sh.frame("after_compound");
});

parity!(store_home_drives_interactive_cd, Fixture::new(), |sh: &mut Live| {
    sh.line("set HOME /usr");
    sh.line("cd; pwd");
    sh.frame("after_cd");
});

parity!(unset_oldpwd_blocks_interactive_cd_minus, Fixture::new(), |sh: &mut Live| {
    sh.line("cd /");
    sh.line("unset OLDPWD");
    sh.line("cd -");
    sh.frame("after_cd_minus");
});

parity!(prefix_assignment_reaches_child_but_does_not_persist, Fixture::new(), |sh: &mut Live| {
    sh.line("TMP_PREFIX=value env | grep TMP_PREFIX");
    sh.frame("with_prefix");
    sh.line("env | grep TMP_PREFIX");
    sh.frame("without_prefix");
});

parity!(
    set_path_affects_command_lookup,
    Fixture::new().executable("bin/mytool", "#!/bin/sh\necho mytool-ran\n"),
    |sh: &mut Live| {
        sh.line("export PATH=$HOME/bin");
        sh.line("mytool");
        sh.frame("after_mytool");
    }
);

parity!(
    which_reflects_exported_path,
    Fixture::new().executable("bin/mytool", "#!/bin/sh\necho mytool-ran\n"),
    |sh: &mut Live| {
        sh.line("export PATH=$HOME/bin");
        sh.line("which mytool");
        sh.frame("after_which");
    }
);

parity!(which_uses_store_path_not_os_env, Fixture::new().dir("empty-bin"), |sh: &mut Live| {
    sh.line("export PATH=$HOME/empty-bin");
    sh.line("which ls");
    sh.frame("after_which_ls");
    sh.line("which /bin/ls");
    sh.frame("after_which_absolute");
});

parity!(tilde_expansion, Fixture::new(), |sh: &mut Live| {
    sh.line("echo ~");
    sh.frame("after_tilde");
});

parity!(
    history_up_arrow,
    Fixture::new().history(&["echo from_global"]),
    |sh: &mut Live| {
        sh.line("echo local_one");
        sh.line("echo local_two");
        sh.keys(key::UP);
        sh.frame("up_1");
        sh.keys(key::UP);
        sh.frame("up_2");
        sh.keys(key::UP);
        sh.frame("up_3");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    history_up_narrow_repaint_clears_wrapped_rows,
    Fixture::new().size(24, 12),
    |sh: &mut Live| {
        sh.line("echo ok");
        sh.line("echo WRAPMARK12345678901234567890");
        sh.line("echo newer");
        sh.repeat(key::UP, 32);
        sh.frame("oldest");
    }
);

parity!(
    history_ctrl_r_search,
    Fixture::new().history(&["echo alpha", "echo beta", "echo gamma"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.frame("search_open");
        sh.text("beta");
        sh.frame("query_beta");
        sh.keys(key::ENTER);
        sh.frame("accepted");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("executed");
    }
);

parity!(
    history_ctrl_r_ignores_later_global_entries,
    Fixture::new().history(&["echo startup"]),
    |sh: &mut Live| {
        let path = sh.home().join(sh.kind().history_file());
        {
            use std::io::Write as _;
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open history")
                .write_all(b"echo later_global\n")
                .expect("append history");
        }
        sh.keys(key::CTRL_R);
        sh.frame("search_open");
        sh.text("later");
        sh.frame("query_later");
        sh.keys(key::ENTER);
        sh.frame("enter_without_match");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    history_ctrl_r_escape_cancels,
    Fixture::new().history(&["echo secret"]),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("secret");
        sh.frame("query_secret");
        sh.keys(key::ESC);
        sh.frame("after_escape");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    history_ctrl_r_narrow_repaint_does_not_stack_rows,
    Fixture::new().history(&["abc1", "abc2", "abc3"]).size(24, 10),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("ab");
        sh.text("c");
        sh.keys(key::BACKSPACE);
        sh.text("c");
        sh.keys(key::DOWN);
        sh.keys(key::UP);
        sh.frame("search");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(
    history_search_selection_preserves_cursor_after_typing,
    Fixture::new()
        .history(&[
            "abcdefghi0",
            "abcdefghi1",
            "abcdefghi2",
            "abcdefghi3",
            "abcdefghi4",
            "abcdefghi5",
            "abcdefghi6",
            "abcdefghi7",
            "abcdefghi8",
            "abcdefghi9",
        ])
        .size(24, 10),
    |sh: &mut Live| {
        let fill = (1..=14).map(|i| format!("echo fill{i:02}")).collect::<Vec<_>>().join("; ");
        sh.line(&fill);
        sh.keys(key::CTRL_R);
        sh.frame("open_search");
        sh.text("abc");
        sh.frame("type_abc");
        sh.keys(key::DOWN);
        sh.frame("select_next");
        sh.text("x");
        sh.frame("type_x");
        sh.keys(key::ESC);
    }
);

parity!(
    history_accept_reanchors_prompt_before_typing,
    Fixture::new()
        .history(&["echo history-one-abcdefghijklmnop", "echo history-two-abcdefghijklmnop"])
        .size(8, 20),
    |sh: &mut Live| {
        let fill = (1..=6).map(|i| format!("echo fill{i:02}")).collect::<Vec<_>>().join("; ");
        sh.line(&fill);
        sh.keys(key::CTRL_R);
        sh.frame("open_search");
        sh.text("history");
        sh.frame("query");
        sh.keys(key::DOWN);
        sh.frame("select_second");
        sh.keys(key::ENTER);
        sh.frame("accept");
        sh.text("x");
        sh.frame("type_after_accept");
        sh.keys_to_prompt(key::CTRL_C);
    }
);

parity!(
    history_ctrl_r_near_bottom_keeps_pager_stable,
    Fixture::new().history(&[
        "hist01", "hist02", "hist03", "hist04", "hist05", "hist06", "hist07", "hist08", "hist09",
        "hist10", "hist11", "hist12",
    ]).size(24, 20),
    |sh: &mut Live| {
        let fill = (1..=14).map(|i| format!("echo fill{i:02}")).collect::<Vec<_>>().join("; ");
        sh.line(&fill);
        sh.keys(key::CTRL_R);
        sh.frame("search");
        sh.keys(key::ESC);
    }
);

parity!(
    history_ctrl_r_scrolls_when_selection_passes_last_visible_entry,
    Fixture::new().history(&[
        "hist01", "hist02", "hist03", "hist04", "hist05", "hist06", "hist07", "hist08", "hist09",
        "hist10", "hist11", "hist12",
    ]).size(8, 20),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.frame("search");
        sh.repeat(key::DOWN, 11);
        sh.frame("scrolled");
        sh.keys(key::ESC);
    }
);

parity!(
    history_ctrl_r_near_bottom_query_edits_do_not_stack_headers,
    Fixture::new().history(&[
        "gh auth login",
        "gh api repos/openai/openai/contents",
        "gh api user",
        "gh pr status",
        "gh api rate_limit",
        "gh api notifications",
        "gh api orgs/openai/repos",
        "gh api repos/openai/openai/pulls",
        "gh api repos/openai/openai/issues",
        "gh api repos/openai/openai/actions/runs",
        "gh api repos/openai/openai/releases",
        "gh api repos/openai/openai/branches",
    ]).size(24, 20),
    |sh: &mut Live| {
        let fill = (1..=14).map(|i| format!("echo fill{i:02}")).collect::<Vec<_>>().join("; ");
        sh.line(&fill);
        sh.keys(key::CTRL_R);
        sh.frame("open");
        for (label, ch) in [("g", "g"), ("h", "h"), ("space", " "), ("a", "a"), ("p", "p"), ("i", "i")] {
            sh.text(ch);
            sh.frame(label);
        }
        sh.keys(key::ESC);
    }
);

parity!(
    tab_completion_files,
    Fixture::new()
        .file("alpha.txt", "")
        .file("bravo.txt", "")
        .file("charlie.txt", ""),
    |sh: &mut Live| {
        sh.text("echo al");
        sh.keys(key::TAB);
        sh.frame("after_tab");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    tab_completion_shows_grid,
    Fixture::new().file("aaa.txt", "").file("aab.txt", "").file("aac.txt", ""),
    |sh: &mut Live| {
        sh.text("echo aa");
        sh.keys(key::TAB);
        sh.frame("grid");
        sh.keys(key::ESC);
        sh.frame("after_escape");
        sh.keys(key::CTRL_U);
        sh.frame("after_kill");
    }
);

parity!(
    tab_completion_first_tab_has_no_selection_second_tab_selects_first,
    Fixture::new().file("aaa.txt", "").file("aab.txt", "").file("aac.txt", ""),
    |sh: &mut Live| {
        sh.text("echo aa");
        sh.keys(key::TAB);
        sh.frame("first_tab");
        sh.keys(key::TAB);
        sh.frame("second_tab");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(
    tab_completion_narrow_repaint_does_not_stack_rows,
    Fixture::new()
        .file("aaa.txt", "")
        .file("aab.txt", "")
        .file("aac.txt", "")
        .size(24, 12),
    |sh: &mut Live| {
        sh.text("echo a");
        sh.keys(key::TAB);
        sh.frame("grid");
        sh.text("a");
        sh.frame("filtered");
        sh.keys(key::DOWN);
        sh.keys(key::DOWN);
        sh.keys(key::UP);
        sh.frame("navigated");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(
    tab_completion_directory,
    Fixture::new().file("mydir/.keep", ""),
    |sh: &mut Live| {
        sh.text("cd my");
        sh.keys(key::TAB);
        sh.frame("after_tab");
        sh.keys_to_prompt(key::ENTER);
        sh.line("pwd");
        sh.frame("after_pwd");
    }
);

parity!(
    tab_completion_escape_restores_typed_prefix,
    Fixture::new().file("alpha.txt", "").file("alpine.txt", ""),
    |sh: &mut Live| {
        sh.text("echo al");
        sh.keys(key::TAB);
        sh.frame("grid");
        sh.keys(key::ESC);
        sh.text("x");
        sh.frame("after_x");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    tab_completion_narrowing_does_not_autoaccept,
    Fixture::new().file("signal.rs", "").file("sys.rs", ""),
    |sh: &mut Live| {
        sh.text("echo s");
        sh.keys(key::TAB);
        sh.frame("grid");
        sh.text("y");
        sh.frame("narrowed");
        sh.keys(key::ESC);
        sh.text("x");
        sh.frame("after_x");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    tab_completion_with_wide_dir_name_restores_prompt_cursor,
    Fixture::new()
        .dir_at("Sync/M/Music/bandcamp/Altered States", 1_700_000_100)
        .dir_at("Sync/M/Music/bandcamp/Another Language", 1_700_000_090)
        .dir_at("Sync/M/Music/bandcamp/Dreamage", 1_700_000_080)
        .dir_at("Sync/M/Music/bandcamp/黑馬河的兒子 The Son of Black Horse River", 1_700_000_070)
        .dir_at("Sync/M/Music/bandcamp/Arigto - Lungs", 1_700_000_060)
        .file_at(
            "Sync/M/Music/bandcamp/01 - Charlotte de Witte - Sehnsucht (Original Mix) [56812652].mp3",
            "",
            1_700_000_050,
        )
        .file_at(
            "Sync/M/Music/bandcamp/Floating Points, Pharoah Sanders & The London Symphony Orchestra - Promises [Movement 6] [FQdLWlvgHOg].m4a",
            "",
            1_700_000_040,
        ),
    |sh: &mut Live| {
        sh.text("/Applications/Play.app/Contents/MacOS/play ~/Sync/M/Music/bandcamp/");
        sh.frame("typed");
        sh.keys(key::TAB);
        sh.frame("first_tab");
        sh.keys(key::TAB);
        sh.frame("second_tab");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(
    completion_resize_rerenders_grid,
    Fixture::new()
        .file("aaa.txt", "")
        .file("aab.txt", "")
        .file("aac.txt", "")
        .size(24, 20),
    |sh: &mut Live| {
        sh.text("echo a");
        sh.keys(key::TAB);
        sh.frame("grid");
        sh.resize(24, 12);
        sh.frame("after_resize");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(
    normal_resize_reanchors_wrapped_prompt,
    Fixture::new().size(12, 80),
    |sh: &mut Live| {
        sh.line("printf '\\n\\n'");
        sh.text("echo resize-test-abcdefghijklmnopqrstuvwxyz");
        sh.frame("before_resize");
        sh.resize(12, 52);
        sh.frame("after_resize");
        sh.text("x");
        sh.frame("after_x");
        sh.keys_to_prompt(key::CTRL_C);
    }
);

parity!(
    history_resize_rerenders_pager,
    Fixture::new().history(&["abc1", "abc2", "abc3"]).size(24, 20),
    |sh: &mut Live| {
        sh.keys(key::CTRL_R);
        sh.text("abc");
        sh.frame("search");
        sh.resize(24, 10);
        sh.frame("after_resize");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(alias_expansion, Fixture::new(), |sh: &mut Live| {
    sh.line("alias g echo git_command");
    sh.line("g hello");
    sh.frame("after_alias");
});

parity!(alias_self_referencing_no_reexpand, Fixture::new(), |sh: &mut Live| {
    sh.line("alias rg rg --hidden -S -g !.git");
    sh.text("rg");
    sh.text(" ");
    sh.frame("first_space");
    sh.text(" ");
    sh.frame("second_space");
    sh.keys_to_prompt(key::CTRL_C);
});

parity!(
    alias_self_referencing_from_config,
    Fixture::new().config("alias rg rg --hidden -S -g !.git\n"),
    |sh: &mut Live| {
        sh.text("rg");
        sh.text(" ");
        sh.frame("first_space");
        sh.text(" ");
        sh.frame("second_space");
        sh.text(" ");
        sh.frame("third_space");
        sh.keys_to_prompt(key::CTRL_C);
    }
);

parity!(alias_self_referencing_exec, Fixture::new(), |sh: &mut Live| {
    sh.line("alias myecho echo --verbose");
    sh.line("myecho hello");
    sh.frame("after_alias");
});

parity!(alias_list, Fixture::new(), |sh: &mut Live| {
    sh.line("alias myalias echo test");
    sh.line("alias");
    sh.frame("after_list");
    sh.line("alias myalias");
    sh.frame("after_show");
    sh.line("alias nosuchalias");
    sh.frame("after_missing");
});

parity!(alias_with_command_substitution, Fixture::new(), |sh: &mut Live| {
    sh.line(r#"alias grt echo "$(echo hello_subst)""#);
    sh.text("grt");
    sh.text(" ");
    sh.frame("expanded");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(alias_preserves_quoted_word, Fixture::new(), |sh: &mut Live| {
    sh.line(r#"alias greet echo "hello world""#);
    sh.line("greet");
    sh.frame("after_alias");
});

parity!(which_builtin, Fixture::new(), |sh: &mut Live| {
    sh.line("w echo");
    sh.frame("after_w");
});

parity!(
    which_external,
    Fixture::new().executable("bin/mytool", "#!/bin/sh\n"),
    |sh: &mut Live| {
        sh.line("export PATH=$HOME/bin");
        sh.line("w mytool");
        sh.frame("after_w");
        sh.line("w nosuchtool");
        sh.frame("after_missing");
        sh.line("w");
        sh.frame("after_no_args");
    }
);

parity!(which_alias, Fixture::new(), |sh: &mut Live| {
    sh.line("alias g git");
    sh.line("w g");
    sh.frame("after_w");
});

parity!(error_status_colors_prompt, Fixture::new(), |sh: &mut Live| {
    sh.line("false");
    sh.frame("after_false");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(nonexistent_command, Fixture::new(), |sh: &mut Live| {
    sh.line("nonexistent_cmd_xyz");
    sh.frame("after_command");
    sh.line("echo $?");
    sh.frame("status");
});

parity!(source_nonexistent_error, Fixture::new(), |sh: &mut Live| {
    sh.line("source foo.sh");
    sh.frame("after_source");
});

parity!(ctrl_l_clears_screen, Fixture::new(), |sh: &mut Live| {
    sh.line("echo before_clear");
    sh.text("echo after_clear");
    sh.keys(key::CTRL_L);
    sh.frame("after_clear");
});

parity!(
    multiline_continuation,
    Fixture::new(),
    |sh: &mut Live| {
        sh.text("echo hello |");
        sh.keys(key::ENTER);
        sh.frame("continuation");
        sh.text("tr a-z A-Z");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(
    multiline_completion_on_continuation_line,
    Fixture::new().executable("bin/upper", "#!/bin/sh\ntr a-z A-Z\n"),
    |sh: &mut Live| {
        sh.text("echo hello |");
        sh.keys(key::ENTER);
        sh.text("./bin/up");
        sh.keys(key::TAB);
        sh.frame("after_tab");
        sh.keys_to_prompt(key::CTRL_C);
    }
);

parity!(
    dir_picker_narrow_repaint_does_not_stack_rows,
    Fixture::new().file("one/.keep", "").file("two/.keep", "").file("three/.keep", "").size(24, 40),
    |sh: &mut Live| {
        sh.line("cd one");
        sh.line("cd ../two");
        sh.line("cd ../three");
        sh.keys(key::CTRL_BACKSPACE);
        sh.frame("picker");
        sh.keys(key::DOWN);
        sh.keys(key::UP);
        sh.frame("navigated");
        sh.keys(key::ESC);
        sh.frame("after_escape");
    }
);

parity!(
    config_file_loaded,
    Fixture::new().config("alias greet echo hello_from_config\n"),
    |sh: &mut Live| {
        sh.line("greet world");
        sh.frame("after_alias");
    }
);

parity!(prompt_shows_cwd, Fixture::new(), |sh: &mut Live| {
    sh.enter();
    sh.wait_prompt();
    sh.frame("after_enter");
});

parity!(cd_minus_goes_back, Fixture::new().file("subdir/.keep", ""), |sh: &mut Live| {
    sh.line("cd subdir");
    sh.frame("in_subdir");
    sh.line("cd -");
    sh.frame("after_cd_minus");
    sh.line("pwd");
    sh.frame("after_pwd");
});

parity!(cd_tilde_subdir, Fixture::new().file("subdir/.keep", ""), |sh: &mut Live| {
    sh.line("cd ~/subdir");
    sh.line("pwd");
    sh.frame("after_pwd");
});

parity!(implicit_cd_quoted_path, Fixture::new().file("space dir/.keep", ""), |sh: &mut Live| {
    sh.line("'space dir'");
    sh.line("pwd");
    sh.frame("after_pwd");
});

parity!(l_tilde_subdir, Fixture::new().file("subdir/file.txt", "hello"), |sh: &mut Live| {
    sh.line("l ~/subdir");
    sh.frame("after_l");
});

parity!(unset_variable, Fixture::new(), |sh: &mut Live| {
    sh.line("set TMPVAR abc");
    sh.line("unset TMPVAR");
    sh.line("echo $TMPVAR");
    sh.frame("after_echo");
});

parity!(
    glob_expansion,
    Fixture::new().file("foo.rs", "").file("bar.rs", "").file("baz.txt", ""),
    |sh: &mut Live| {
        sh.line("echo *.rs");
        sh.frame("after_glob");
    }
);

parity!(
    l_glob_expansion,
    Fixture::new().file("rust-toolchain.toml", "").file("toolbox.txt", "").file("Cargo.toml", ""),
    |sh: &mut Live| {
        sh.line("l *tool*");
        sh.frame("after_l");
    }
);

parity!(quoted_string_preserves_spaces, Fixture::new(), |sh: &mut Live| {
    sh.line("echo \"hello   world\"");
    sh.frame("after_echo");
});

parity!(single_quotes_no_expansion, Fixture::new(), |sh: &mut Live| {
    sh.line("set FOO bar");
    sh.line("echo '$FOO'");
    sh.frame("after_echo");
});

parity!(history_persisted_across_commands, Fixture::new(), |sh: &mut Live| {
    sh.line("/bin/echo unique_cmd_12345");
    sh.keys(key::UP);
    sh.frame("recalled");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
    sh.effect_history("history");
});

parity!(history_help, Fixture::new(), |sh: &mut Live| {
    sh.line("history -h");
    sh.frame("after_help");
    sh.line("history bogus");
    sh.frame("after_unknown");
});

parity!(
    history_autosuggest_ignores_later_global_entries,
    Fixture::new().history(&["echo startup"]),
    |sh: &mut Live| {
        let path = sh.home().join(sh.kind().history_file());
        {
            use std::io::Write as _;
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .expect("open history")
                .write_all(b"echo later_global\n")
                .expect("append history");
        }
        sh.keys(key::CTRL_R);
        sh.keys(key::ESC);
        sh.frame("after_search");
        sh.text("echo l");
        sh.keys(key::RIGHT);
        sh.frame("after_right");
        sh.keys_to_prompt(key::ENTER);
        sh.frame("after_enter");
    }
);

parity!(true_and_false_builtins, Fixture::new(), |sh: &mut Live| {
    sh.line("true && echo ok");
    sh.line("false && echo bad || echo good");
    sh.frame("after_lists");
});

parity!(bracketed_paste_over_limit_rejected, Fixture::new(), |sh: &mut Live| {
    sh.keys(&paste(&"x".repeat(9000)));
    sh.frame("after_paste");
    sh.keys(key::CTRL_U);
    sh.line("echo ok");
    sh.frame("still_responsive");
});

parity!(bracketed_paste_large_document_rejected, Fixture::new(), |sh: &mut Live| {
    let document = format!("# Agent Guide\n{}", "This line is filler for a large paste.\n".repeat(400));
    sh.keys(&paste(&document));
    sh.frame("after_paste");
    sh.keys(key::CTRL_U);
    sh.line("echo ok");
    sh.frame("still_responsive");
});

parity!(bracketed_paste_under_limit_accepted, Fixture::new(), |sh: &mut Live| {
    sh.keys(&paste("echo hello world"));
    sh.frame("after_paste");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

parity!(bracketed_paste_exactly_at_limit_accepted, Fixture::new(), |sh: &mut Live| {
    sh.keys(&paste(&"x".repeat(8192)));
    sh.frame("after_paste");
    sh.text(" ok");
    sh.keys_to_prompt(key::CTRL_C);
    sh.frame("after_cancel");
});

parity!(bracketed_paste_one_byte_over_limit_rejected, Fixture::new(), |sh: &mut Live| {
    sh.keys(&paste(&"x".repeat(8193)));
    sh.frame("after_paste");
});

parity!(bracketed_paste_multiline_joins_lines, Fixture::new(), |sh: &mut Live| {
    sh.keys(&paste("echo first\necho second"));
    sh.frame("after_paste");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("after_enter");
});

const READY_SLEEP: &str = "#!/bin/sh\necho ready\nsleep 60\n";

parity!(
    job_suspend_and_resume,
    Fixture::new().executable("bin/job", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.frame("stopped");
        sh.line_until("fg", "resuming:");
        sh.frame("resumed");
        sh.keys_to_prompt(key::CTRL_C);
        sh.frame("after_interrupt");
        sh.line("echo alive");
        sh.frame("alive");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(fg_without_a_job_reports_an_error, Fixture::new(), |sh: &mut Live| {
    sh.line("fg");
    sh.frame("after_fg");
    sh.line("echo $?");
    sh.frame("status");
});

parity!(
    exit_with_a_suspended_job_warns,
    Fixture::new().executable("bin/job", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.line("exit");
        sh.frame("first_exit");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(
    ctrl_d_with_a_suspended_job_warns_before_forcing,
    Fixture::new().executable("bin/job", READY_SLEEP),
    |sh: &mut Live| {
        sh.line_until("./bin/job", "ready");
        sh.keys_to_prompt(key::CTRL_Z);
        sh.keys(key::CTRL_D);
        sh.frame_rows_containing("first_ctrl_d", "suspended job");
        sh.keys(key::CTRL_D);
        sh.expect_exit("exit_status");
    }
);

// Redirections, pipes, and statuses.

parity!(redirect_append_and_stdin, Fixture::new(), |sh: &mut Live| {
    sh.line("echo first > f.txt");
    sh.line("echo second >> f.txt");
    sh.line("cat f.txt");
    sh.frame("after_cat");
    sh.line("cat < f.txt");
    sh.frame("after_stdin");
    sh.effect_file("f.txt", "f.txt");
});

parity!(redirect_stderr_forms, Fixture::new(), |sh: &mut Live| {
    sh.line("cat missing.txt 2> err.txt");
    sh.line("cat err.txt");
    sh.frame("after_stderr_file");
    sh.line("cat missing.txt 2>> err.txt");
    sh.line("cat err.txt");
    sh.frame("after_stderr_append");
    sh.line("cat missing.txt 2>&1");
    sh.frame("after_dup");
    sh.line("cat missing.txt > both.txt 2>&1");
    sh.line("cat both.txt");
    sh.frame("after_both");
});

parity!(redirect_to_unwritable_path, Fixture::new(), |sh: &mut Live| {
    sh.line("echo hi > /nonexistent-dir/file");
    sh.frame("after_failed_redirect");
    sh.line("echo $?");
    sh.frame("status");
});

parity!(exit_status_of_external_commands, Fixture::new(), |sh: &mut Live| {
    sh.line("sh -c 'exit 7'");
    sh.line("echo $?");
    sh.frame("exit_7");
    sh.line("sh -c 'kill -TERM $$'");
    sh.line("echo $?");
    sh.frame("signal");
    sh.line("false");
    sh.line("echo $?");
    sh.frame("false");
    sh.line("true");
    sh.line("echo $?");
    sh.frame("true");
});

parity!(
    exit_status_of_unrunnable_commands,
    Fixture::new().file("notexec.sh", "echo hi\n").dir("adir"),
    |sh: &mut Live| {
        sh.line("./notexec.sh");
        sh.frame("not_executable");
        sh.line("echo $?");
        sh.frame("status_126");
        sh.line("nosuchcommand_xyz");
        sh.line("echo $?");
        sh.frame("status_127");
        sh.line("./nosuchdir/cmd");
        sh.frame("missing_path");
    }
);

parity!(pipeline_status_is_the_rightmost_failure, Fixture::new(), |sh: &mut Live| {
    sh.line("sh -c 'exit 3' | cat");
    sh.line("echo $?");
    sh.frame("first_fails");
    sh.line("echo x | sh -c 'exit 4'");
    sh.line("echo $?");
    sh.frame("last_fails");
    sh.line("sh -c 'exit 5' | sh -c 'exit 6'");
    sh.line("echo $?");
    sh.frame("both_fail");
});

parity!(and_or_list_statuses, Fixture::new(), |sh: &mut Live| {
    sh.line("false && echo no; echo $?");
    sh.frame("and_short_circuit");
    sh.line("true || echo no; echo $?");
    sh.frame("or_short_circuit");
    sh.line("false || false || echo third");
    sh.frame("chain");
    sh.line("echo a; echo b; echo c");
    sh.frame("sequence");
});

// Quoting, comments, and expansion.

parity!(quoting_forms, Fixture::new(), |sh: &mut Live| {
    sh.line("echo 'a  b' \"c   d\" e\\ f");
    sh.frame("mixed_quotes");
    sh.line("echo \"a\"b'c'");
    sh.frame("adjacent");
    sh.line("echo \"\" x");
    sh.frame("empty_argument");
    sh.line("echo \"a\\\"b\" 'c\\d'");
    sh.frame("escapes");
    sh.line("echo \\$HOME '$HOME' \"$HOME\"");
    sh.frame("dollar_forms");
});

parity!(comments, Fixture::new(), |sh: &mut Live| {
    sh.line("echo a # b c");
    sh.frame("trailing_comment");
    sh.line("echo a#b");
    sh.frame("hash_inside_word");
    sh.line("# only a comment");
    sh.frame("comment_only");
    sh.line("echo 'a # b' \"c # d\"");
    sh.frame("hash_in_quotes");
});

parity!(continuation_lines, Fixture::new(), |sh: &mut Live| {
    sh.text("echo one \\");
    sh.keys(key::ENTER);
    sh.frame("after_backslash");
    sh.text("two");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("joined");
    sh.text("true &&");
    sh.keys(key::ENTER);
    sh.frame("after_and");
    sh.text("echo and-done");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("and_done");
    sh.text("false ||");
    sh.keys(key::ENTER);
    sh.text("echo or-done");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("or_done");
    sh.text("echo \"open");
    sh.keys(key::ENTER);
    sh.frame("after_open_quote");
    sh.text("close\"");
    sh.keys_to_prompt(key::ENTER);
    sh.frame("quote_done");
});

parity!(tilde_and_variable_expansion, Fixture::new().dir("sub"), |sh: &mut Live| {
    sh.line("echo ~ ~/sub ~x");
    sh.frame("tilde");
    sh.line("set NAME world");
    sh.line("echo hello-$NAME ${NAME} $NAME-x $NOPE| cat");
    sh.frame("variables");
    sh.line("echo $((1 + 2 * 3)) $(( (1+2) * 3 ))");
    sh.frame("arithmetic");
});

parity!(command_substitution_forms, Fixture::new(), |sh: &mut Live| {
    sh.line("echo a$(echo b)c `echo d`e");
    sh.frame("basic");
    sh.line("echo $(echo $(echo nested))");
    sh.frame("nested");
    sh.line("echo \"$(echo   spaced   out)\" $(echo   spaced   out)");
    sh.frame("word_splitting");
    sh.line("echo $(sh -c 'exit 3') after");
    sh.frame("failing_substitution");
    sh.line("echo $?");
    sh.frame("status");
});

parity!(
    glob_forms,
    Fixture::new()
        .file("a.txt", "")
        .file("b.txt", "")
        .file("c.log", "")
        .file(".hidden.txt", "")
        .file("d/e.txt", "")
        .file("d/f/g.txt", ""),
    |sh: &mut Live| {
        sh.line("echo *.txt");
        sh.frame("star");
        sh.line("echo ?.txt");
        sh.frame("question");
        sh.line("echo [ab].txt");
        sh.frame("class");
        sh.line("echo .*.txt");
        sh.frame("hidden_explicit");
        sh.line("echo d/*");
        sh.frame("subdirectory");
    }
);

// Builtins.

parity!(cd_forms, Fixture::new().dir("a/b").file("plain.txt", ""), |sh: &mut Live| {
    sh.line("cd a");
    sh.frame("cd_a");
    sh.line("cd b");
    sh.line("cd ../..");
    sh.frame("cd_up");
    sh.line("cd -");
    sh.frame("cd_dash");
    sh.line("cd");
    sh.frame("cd_home");
    sh.line("cd nonexistent");
    sh.frame("cd_missing");
    sh.line("cd plain.txt");
    sh.frame("cd_file");
    sh.line("cd a b");
    sh.frame("cd_too_many");
    sh.line("cd ~/a");
    sh.line("cd $HOME");
    sh.frame("cd_variable");
});

parity!(implicit_cd_forms, Fixture::new().dir("a/b/c").executable("run", "#!/bin/sh\n"), |sh: &mut Live| {
    sh.line("a");
    sh.frame("implicit_cd");
    sh.line("..");
    sh.frame("dotdot");
    sh.line("a/b/c");
    sh.line("...");
    sh.frame("three_dots");
    sh.line("....");
    sh.frame("four_dots");
    sh.line("~");
    sh.frame("tilde_alone");
    sh.line("./a");
    sh.frame("relative");
    sh.line("nonexistent_dir_xyz");
    sh.frame("not_a_dir");
});

parity!(echo_flags, Fixture::new(), |sh: &mut Live| {
    sh.line("echo -n no-newline");
    sh.frame("dash_n");
    sh.line("echo -e 'a\\tb\\nc'");
    sh.frame("dash_e");
    sh.line("echo 'a\\tb'");
    sh.frame("no_escapes");
    sh.line("echo -E 'a\\tb'");
    sh.frame("dash_capital_e");
    sh.line("echo");
    sh.frame("empty");
});

parity!(type_and_which_forms, Fixture::new().executable("bin/tool", "#!/bin/sh\n"), |sh: &mut Live| {
    sh.line("alias ll l");
    sh.line("type ll");
    sh.frame("type_alias");
    sh.line("which cd");
    sh.frame("which_builtin");
    sh.line("type nosuchthing_xyz");
    sh.frame("type_missing");
    sh.line("echo $?");
    sh.frame("status");
    sh.line("which /bin/sh");
    sh.frame("which_absolute");
    sh.line("w");
    sh.frame("w_no_args");
});

parity!(alias_edge_cases, Fixture::new(), |sh: &mut Live| {
    sh.line("alias a echo one");
    sh.line("alias b a two");
    sh.line("b");
    sh.frame("alias_of_alias");
    sh.line("alias e echo");
    sh.line("e   spaced    args");
    sh.frame("spacing");
    sh.line("alias q 'echo quoted words'");
    sh.line("q");
    sh.frame("quoted_alias");
    sh.line("alias ll");
    sh.frame("show_one");
    sh.line("alias cd echo redefined");
    sh.line("cd");
    sh.frame("alias_over_builtin");
});

parity!(exit_codes, Fixture::new(), |sh: &mut Live| {
    sh.text("exit 3");
    sh.enter();
    sh.expect_exit("exit_3");
});

parity!(exit_code_wraps_modulo_256, Fixture::new(), |sh: &mut Live| {
    sh.text("exit 300");
    sh.enter();
    sh.expect_exit("exit_300");
});

parity!(exit_code_ignores_garbage, Fixture::new(), |sh: &mut Live| {
    sh.line("false");
    sh.text("exit notanumber");
    sh.enter();
    sh.expect_exit("exit_garbage");
});

parity!(exit_without_argument_is_success, Fixture::new(), |sh: &mut Live| {
    sh.line("false");
    sh.text("exit");
    sh.enter();
    sh.expect_exit("exit_plain");
});

parity!(ctrl_d_exits_success_after_failure, Fixture::new(), |sh: &mut Live| {
    sh.line("false");
    sh.keys(key::CTRL_D);
    sh.expect_exit("ctrl_d_status");
});

parity!(clear_builtin, Fixture::new(), |sh: &mut Live| {
    sh.line("echo before");
    sh.line("c");
    sh.frame("after_clear");
    sh.line("echo after");
    sh.frame("after_echo");
});

parity!(
    l_forms,
    Fixture::new()
        .file("Beta.txt", "bb")
        .file("alpha.txt", "a")
        .file("Zeta.txt", "z")
        .executable("run.sh", "#!/bin/sh\n")
        .symlink("link", "alpha.txt")
        .symlink("dirlink", "sub")
        .dir("sub")
        .file("sub/inner.txt", "i")
        .file(".dotfile", ""),
    |sh: &mut Live| {
        sh.line("l");
        sh.frame("l_default");
        sh.line("l sub");
        sh.frame("l_dir");
        sh.line("l alpha.txt");
        sh.frame("l_file");
        sh.line("l sub alpha.txt");
        sh.frame("l_two");
        sh.line("l nonexistent");
        sh.frame("l_missing");
        sh.line("echo $?");
        sh.frame("status");
        sh.line("l link");
        sh.frame("l_symlink");
    }
);

parity!(history_command_forms, Fixture::new(), |sh: &mut Live| {
    sh.line("echo one");
    sh.line("echo two");
    sh.line("history");
    sh.frame("history_list");
    sh.effect_history("after_history");
});

parity!(environment_builtin_forms, Fixture::new(), |sh: &mut Live| {
    sh.line("export A_VAR=1 B_VAR=two");
    sh.line("export | grep _VAR");
    sh.frame("export_list");
    sh.line("set C_VAR");
    sh.line("env | grep C_VAR");
    sh.frame("set_empty");
    sh.line("set 9bad value");
    sh.frame("set_invalid");
    sh.line("unset A_VAR B_VAR C_VAR");
    sh.line("env | grep _VAR");
    sh.frame("after_unset");
    sh.line("export 9bad=1");
    sh.frame("export_invalid");
    sh.line("FOO=bar");
    sh.line("echo $FOO");
    sh.line("env | grep FOO");
    sh.frame("plain_assignment_not_exported");
    sh.line("export FOO");
    sh.line("env | grep FOO");
    sh.frame("export_existing");
});

parity!(alias_list_in_pipeline, Fixture::new(), |sh: &mut Live| {
    sh.line("alias only echo one");
    sh.line("alias | cat");
    sh.frame("list_in_pipeline");
});

// denv: automatic `.envrc` / `.env` loading.

const ENVRC_LOADED: &str = "export DENV_TEST_VAR='loaded'\n";

/// Wide enough that messages naming absolute paths never wrap, whatever the
/// platform's temporary directory looks like.
const WIDE: u16 = 200;

parity!(
    denv_loads_allowed_envrc_on_cd,
    Fixture::new().size(24, WIDE).file("project/.envrc", ENVRC_LOADED).allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_loads_dotenv_on_cd_without_allow,
    Fixture::new().size(24, WIDE).file("project/.env", "DENV_TEST_VAR=loaded\n"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_unloads_on_leave,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "export DENV_TEST_VAR='active'\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("loaded");
        sh.line("cd ..");
        sh.frame("after_leave");
        sh.line("echo =$DENV_TEST_VAR=");
        sh.frame("unloaded");
    }
);

parity!(
    denv_allow_applies_env,
    Fixture::new().size(24, WIDE).file("project/.envrc", "export DENV_TEST_VAR='allowed'\n"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("blocked");
        sh.line("echo =$__DENV_DIRTY=");
        sh.frame("dirty");
        sh.line("denv allow");
        sh.frame("after_allow");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_deny_removes_env_and_marks_dirty,
    Fixture::new().size(24, WIDE).file("project/.envrc", ENVRC_LOADED).allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("denv deny");
        sh.frame("after_deny");
        sh.line("echo =$DENV_TEST_VAR=:$__DENV_DIRTY=");
        sh.frame("state");
    }
);

parity!(
    denv_startup_loads_dotenv_in_initial_cwd,
    Fixture::new().size(24, WIDE).file("project/.env", "DENV_TEST_VAR=from_startup\n").cwd("project"),
    |sh: &mut Live| {
        sh.frame("startup");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_startup_loads_allowed_envrc_in_initial_cwd,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "export DENV_TEST_VAR='from_startup'\n")
        .allow_envrc("project/.envrc")
        .cwd("project"),
    |sh: &mut Live| {
        sh.frame("startup");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_dotenv_overrides_envrc,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "export SHARED='from_envrc'\nexport ENVRC_ONLY='1'\n")
        .file("project/.env", "SHARED=from_dotenv\nDOTENV_ONLY=1\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo $SHARED $ENVRC_ONLY $DOTENV_ONLY");
        sh.frame("values");
    }
);

parity!(
    denv_reload_after_reallow_picks_up_envrc_edit,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "export DENV_TEST_VAR='old'\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.rewrite_file("project/.envrc", "export DENV_TEST_VAR='updated'\n", 10);
        sh.line("denv allow");
        sh.line("denv reload");
        sh.frame("after_reload");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_updated");
    }
);

parity!(
    denv_edit_envrc_invalidates_trust,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "export DENV_TEST_VAR='old'\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.rewrite_file("project/.envrc", "export DENV_TEST_VAR='changed'\n", 10);
        sh.line("denv reload");
        sh.frame("after_reload");
        sh.line("echo =$DENV_TEST_VAR=:$__DENV_DIRTY=");
        sh.frame("state");
    }
);

parity!(
    denv_restores_preexisting_var_on_leave,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "export EXISTING='inside'\n")
        .allow_envrc("project/.envrc")
        .env("EXISTING", "outside"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("echo $EXISTING");
        sh.frame("inside");
        sh.line("cd ..");
        sh.line("echo $EXISTING");
        sh.frame("outside");
    }
);

parity!(
    denv_path_add_relative_dir,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "PATH_add bin\n")
        .executable("project/bin/tool", "#!/bin/sh\nexit 0\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("echo $PATH");
        sh.frame("path");
    }
);

parity!(
    denv_dotenv_helper_loads_env_file,
    Fixture::new().size(24, WIDE)
        .file("project/.envrc", "dotenv\nexport AFTER='1'\n")
        .file("project/.env", "FROM_ENV=loaded\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        // The reference shell's embedded interpreter warns about `set -a`;
        // a real sh does not, so compare only what the variables ended up as.
        sh.line("c");
        sh.line("echo $FROM_ENV $AFTER");
        sh.frame("values");
    }
);

parity!(
    denv_allow_requires_envrc,
    Fixture::new().size(24, WIDE).file("project/.env", "DENV_TEST_VAR=loaded\n"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("denv allow");
        sh.frame("after_allow");
        sh.line("denv bogus");
        sh.frame("usage");
        sh.line("echo $?");
        sh.frame("status");
    }
);

parity!(
    history_peer_commands_stay_out_of_recall,
    Fixture::new().history(&["echo startup"]),
    |sh: &mut Live| {
        let mut peer = sh.spawn_peer();
        peer.line("echo from_peer");
        sh.enter();
        sh.wait_prompt();
        sh.keys(key::UP);
        sh.frame("up_shows_only_startup_history");
        sh.keys(key::CTRL_U);
        sh.keys(key::CTRL_R);
        sh.text("peer");
        sh.frame("search_finds_nothing_from_peer");
        sh.keys(key::ESC);
        sh.absorb(peer, "peer");
    }
);

parity!(
    history_new_shell_recalls_what_earlier_shells_ran,
    Fixture::new(),
    |sh: &mut Live| {
        let mut first = sh.spawn_peer();
        first.line("echo ran_in_first");
        first.line("echo also_in_first");
        first.text("exit");
        first.enter();
        first.expect_exit("first_exit");
        sh.absorb(first, "first");

        let mut later = sh.spawn_peer();
        later.keys(key::UP);
        later.frame("newest");
        later.keys(key::UP);
        later.frame("older");
        sh.absorb(later, "later");
        sh.effect_listing("data_dir", &format!(".local/share/{}", sh.kind().label()));
    }
);

parity!(
    history_reset_reaches_running_shells,
    Fixture::new().history(&["echo old_one", "echo old_two"]),
    |sh: &mut Live| {
        let mut peer = sh.spawn_peer();
        peer.line("history reset");
        sh.enter();
        sh.wait_prompt();
        sh.keys(key::CTRL_R);
        sh.frame("search_after_reset");
        sh.keys(key::ESC);
        sh.keys(key::UP);
        sh.frame("up_after_reset");
        sh.effect_history("history_file");
        sh.absorb(peer, "peer");
    }
);

parity!(
    history_shells_share_one_log,
    Fixture::new(),
    |sh: &mut Live| {
        let mut peer = sh.spawn_peer();
        sh.line("echo from_first");
        peer.line("echo from_second");
        sh.line("echo from_first_again");
        sh.effect_history("shared_log");
        sh.absorb(peer, "peer");
    }
);

parity!(
    history_repeated_command_moves_to_latest_use_across_shells,
    Fixture::new(),
    |sh: &mut Live| {
        let mut first = sh.spawn_peer();
        let mut second = sh.spawn_peer();
        first.line("echo dup");
        second.line("echo other");
        second.line("echo dup");
        first.text("exit");
        first.enter();
        first.expect_exit("first_exit");
        second.text("exit");
        second.enter();
        second.expect_exit("second_exit");
        sh.absorb(first, "first");
        sh.absorb(second, "second");

        let mut third = sh.spawn_peer();
        third.keys(key::UP);
        third.frame("newest");
        third.keys(key::UP);
        third.frame("older");
        third.keys(key::UP);
        third.frame("nothing_older");
        sh.absorb(third, "third");
    }
);

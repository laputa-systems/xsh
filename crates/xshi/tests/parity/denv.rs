//! denv scenarios for the ish `tests/pty.rs` cases that need trust on both
//! files, helper rejection, active-record cleanup, and the prompt-observable
//! cases from ish's `tests/denv_environment.rs`. Recorded from ish like every
//! other parity scenario.

use super::scenarios::parity;
use super::{Fixture, Live, ShellKind, key, run_scenario};

/// Where the shell under test keeps its denv records.
fn denv_data_dir(sh: &Live) -> &'static str {
    match sh.kind() {
        ShellKind::Xshi => ".local/share/xshi/denv",
        ShellKind::Ish => ".local/share/denv",
    }
}

parity!(
    denv_dotenv_loads_only_after_allow,
    Fixture::new()
        .size(24, 200)
        .file("project/.env", "DENV_TEST_VAR=loaded\nPATH=/nonexistent\n"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo =$DENV_TEST_VAR= $PATH");
        sh.frame("unapproved");
        sh.line("denv allow");
        sh.frame("after_allow");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_startup_loads_allowed_dotenv_in_initial_cwd,
    Fixture::new()
        .size(24, 200)
        .file("project/.env", "DENV_TEST_VAR=from_startup\n")
        .allow_envrc("project/.env")
        .cwd("project"),
    |sh: &mut Live| {
        sh.frame("startup");
        sh.line("echo $DENV_TEST_VAR");
        sh.frame("var_loaded");
    }
);

parity!(
    denv_dotenv_overrides_allowed_envrc,
    Fixture::new()
        .size(24, 200)
        .file(
            "project/.envrc",
            "export SHARED='from_envrc'\nexport ENVRC_ONLY='1'\n"
        )
        .file("project/.env", "SHARED=from_dotenv\nDOTENV_ONLY=1\n")
        .allow_envrc("project/.envrc")
        .allow_envrc("project/.env"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo $SHARED $ENVRC_ONLY $DOTENV_ONLY");
        sh.frame("values");
    }
);

parity!(
    denv_exit_removes_the_active_record,
    Fixture::new()
        .size(24, 200)
        .file("project/.envrc", "export DENV_TEST_VAR='active'\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("loaded");
        sh.text("exit");
        sh.enter();
        sh.expect_exit("exit_status");
        let dir = denv_data_dir(sh);
        sh.effect_listing("records_after_exit", dir);
    }
);

parity!(
    denv_ctrl_d_removes_the_active_record,
    Fixture::new()
        .size(24, 200)
        .file("project/.envrc", "export DENV_TEST_VAR='active'\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("loaded");
        sh.keys(key::CTRL_D);
        sh.expect_exit("exit_status");
        let dir = denv_data_dir(sh);
        sh.effect_listing("records_after_exit", dir);
    }
);

parity!(
    denv_dotenv_helper_rejects_the_entire_candidate,
    Fixture::new()
        .size(24, 200)
        .file(
            "project/.envrc",
            "export BEFORE=1\ndotenv || true\nexport AFTER=1\n"
        )
        .file("project/.env", "FROM_ENV=loaded\n")
        .allow_envrc("project/.envrc")
        .allow_envrc("project/.env"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo =$FROM_ENV=:$BEFORE=:$AFTER=:$__DENV_DIRTY=");
        sh.frame("state");
        sh.line("echo still-usable");
        sh.frame("usable");
    }
);

parity!(
    denv_source_up_is_rejected,
    Fixture::new()
        .size(24, 200)
        .file(
            "parent/.envrc",
            "export PARENT_VALUE=old\nprintf touched > parent-marker\n",
        )
        .file(
            "parent/project/.envrc",
            "export CANDIDATE=before\nsource_up || true\nexport CANDIDATE=after\n",
        )
        .allow_envrc("parent/.envrc")
        .allow_envrc("parent/project/.envrc"),
    |sh: &mut Live| {
        sh.line("cd parent/project");
        sh.frame("after_cd");
        sh.line("echo =$PARENT_VALUE=:$CANDIDATE=:$__DENV_DIRTY=");
        sh.frame("state");
        sh.effect_listing("project_entries", "parent/project");
        sh.effect_listing("parent_entries", "parent");
    }
);

parity!(
    denv_source_env_is_rejected,
    Fixture::new()
        .size(24, 200)
        .file(
            "project/.envrc",
            "export BEFORE=1\nsource_env shared || true\nexport AFTER=1\n",
        )
        .file("project/shared", "export SHARED=original\n")
        .allow_envrc("project/.envrc"),
    |sh: &mut Live| {
        sh.rewrite_file(
            "project/shared",
            "export SHARED=edited\nprintf touched > shared-marker\n",
            10,
        );
        sh.line("cd project");
        sh.frame("after_cd");
        sh.line("echo =$BEFORE=:$AFTER=:$SHARED=:$__DENV_DIRTY=");
        sh.frame("state");
        sh.effect_listing("project_entries", "project");
    }
);

parity!(
    denv_allow_without_any_env_file_fails,
    Fixture::new()
        .size(24, 200)
        .file("project/note.txt", "x\n"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("denv allow");
        sh.frame("after_allow");
    }
);

parity!(
    denv_dotenv_keeps_text_as_data_and_revokes_edits,
    Fixture::new()
        .size(24, 200)
        .file("project/.envrc", "export DENV_PAIR=envrc\n")
        .file(
            "project/.env",
            "DENV_PAIR=dotenv\nDENV_LITERAL=$(touch marker); `touch marker`; \"quoted\"\n",
        )
        .allow_envrc("project/.envrc")
        .allow_envrc("project/.env"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("echo $DENV_PAIR $DENV_LITERAL");
        sh.frame("loaded");
        sh.effect_listing("project_entries", "project");
        sh.rewrite_file("project/.env", "DENV_PAIR=edited\n", 10);
        sh.line("denv reload");
        sh.frame("blocked");
        sh.line("echo =$DENV_PAIR=:$DENV_LITERAL=:$__DENV_DIRTY=");
        sh.frame("rejected");
        sh.line("denv allow");
        sh.line("echo $DENV_PAIR");
        sh.frame("edited");
        sh.line("denv deny");
        sh.frame("denied");
        sh.line("echo =$DENV_PAIR=:$__DENV_DIRTY=");
        sh.frame("denied_state");
    }
);

parity!(
    denv_envrc_sees_the_selected_environment,
    Fixture::new()
        .size(24, 200)
        .file(
            "project/.envrc",
            "#!/bin/sh\nexport CAPTURED=\"$DENV_PROBE\"\n",
        )
        .file("project/.env", "DENV_PROBE=from_dotenv\n")
        .allow_envrc("project/.envrc")
        .allow_envrc("project/.env")
        .env("DENV_PROBE", "from_process"),
    |sh: &mut Live| {
        sh.line("cd project");
        sh.line("echo $CAPTURED $DENV_PROBE");
        sh.frame("values");
    }
);

parity!(
    denv_restores_unexported_and_empty_values_on_leave,
    Fixture::new()
        .size(24, 200)
        .file(
            "project/.env",
            "DENV_LAYER=first\nDENV_EMPTY=changed\nDENV_LOCAL=changed\n",
        )
        .allow_envrc("project/.env")
        .env("DENV_LAYER", "base"),
    |sh: &mut Live| {
        sh.line("DENV_EMPTY=");
        sh.line("DENV_LOCAL=base");
        sh.line("cd project");
        sh.line("echo =$DENV_LAYER=:$DENV_EMPTY=:$DENV_LOCAL=");
        sh.frame("inside");
        sh.line("cd ..");
        sh.line("echo =$DENV_LAYER=:$DENV_EMPTY=:$DENV_LOCAL=");
        sh.frame("outside");
    }
);

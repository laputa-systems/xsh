type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "", diagnostics: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mknod.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C", UUTILS_DIAG: diagnostics}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_mknod_fifo_and_invalid_device_number { |ctx|
  let root = test.temp_dir(ctx, name: "mknod")?
  assert run_applet(ctx, root, ["-m", "640", "pipe", "p"])?.status == 0
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o640
  assert run_applet(ctx, root, ["node", "c", "bad", "3"])?.status == 1
  assert ! fp"{root}/node".exists()?
}


test test_mknod_type_mnemonics_and_operand_diagnostics { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-types")?
  assert run_applet(ctx, root, ["pipe", "pipe"])?.status == 0
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert "Special files require major and minor device numbers." in run_applet(ctx, root, ["node", "c"])?.stderr
  assert "Fifos do not have major and minor device numbers." in run_applet(ctx, root, ["other", "p", "1", "2"])?.stderr
}


test test_mknod_invalid_device_numbers_identify_major_and_minor { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-invalid-device-number")?
  for args in [["node", "c", "c", "1"], ["node", "c", "1", "c"], ["node", "c", "4294967296", "1"]] {
    let result = run_applet(ctx, root, args)?
    let invalid = if args[2] == "1" { args[3] } else { args[2] }
    assert result.status == 1
    let component = if args[2] == "1" { "minor" } else { "major" }
    assert result.stderr == f"mknod: invalid {component} device number '{invalid}'\n", result.stderr
    assert ! fp"{root}/node".exists()?
  }
}

test test_mknod_unknown_long_option_reports_gnu_usage { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-unknown-option")?
  let result = run_applet(ctx, root, ["--foo"])?
  assert result.status == 1
  assert result.stderr == "mknod: unrecognized option '--foo'\nTry 'mknod --help' for more information.\n", result.stderr
  assert result.stdout == ""
}

test test_mknod_invalid_mode_diagnostic_marks_the_bad_operator { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-mode-diagnostic")?
  let result = run_applet(ctx, root, ["-m", "u+rw?", "some_node", "p"], diagnostics: "always")?
  assert result.status == 1
  assert "mknod:1:8" in result.stderr, result.stderr
  assert " 1 │ -m u+rw? some_node p" in result.stderr, result.stderr
}

test test_mknod_context_flag_without_value_is_a_silent_no_op { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-context-flag")?
  for args in [["-Z", "-m", "640", "pipe", "p"], ["--context", "-m", "640", "pipe2", "p"], ["--co", "-m", "640", "pipe3", "p"]] {
    let result = run_applet(ctx, root, args)?
    assert result.status == 0, result.stderr
    assert result.stderr == ""
    assert result.stdout == ""
  }
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe2")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe3")?.kind == "fifo"
}

test test_mknod_context_value_warns_and_still_creates_node { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-context-value")?
  let warning = "mknod: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  for args in [["--context=unconfined_u:object_r:user_tmp_t:s0", "pipe", "p"], ["--context=", "pipe2", "p"], ["--cont=x", "pipe3", "p"], ["-Z", "--context=x", "pipe4", "p"]] {
    let result = run_applet(ctx, root, args)?
    assert result.status == 0, result.stderr
    assert result.stderr == warning, result.stderr
    assert result.stdout == ""
  }
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe2")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe3")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe4")?.kind == "fifo"
}

test test_mknod_context_warning_order_follows_the_command_line { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-context-order")?
  let warning = "mknod: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  let before_help = run_applet(ctx, root, ["--context=x", "--help"])?
  assert before_help.status == 0
  assert before_help.stderr == warning, before_help.stderr
  assert before_help.stdout.starts_with("Usage: mknod"), before_help.stdout
  let after_help = run_applet(ctx, root, ["--help", "--context=x"])?
  assert after_help.status == 0
  assert after_help.stderr == ""
  assert after_help.stdout.starts_with("Usage: mknod"), after_help.stdout
  let before_mode_error = run_applet(ctx, root, ["--context=x", "-m", "bad", "node", "p"])?
  assert before_mode_error.status == 1
  assert before_mode_error.stderr.starts_with(warning), before_mode_error.stderr
  assert "invalid mode" in before_mode_error.stderr, before_mode_error.stderr
}

test test_mknod_context_text_after_mode_is_the_mode_value { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-context-as-mode")?
  let result = run_applet(ctx, root, ["-m", "--context=x", "node", "p"])?
  assert result.status == 1
  assert "warning" not in result.stderr, result.stderr
  assert "invalid mode" in result.stderr, result.stderr
  assert ! fp"{root}/node".exists()?
}

test test_mknod_missing_minor_omits_the_device_number_hint { |ctx|
  let root = test.temp_dir(ctx, name: "mknod-missing-minor")?
  let result = run_applet(ctx, root, ["node", "c", "1"])?
  assert result.status == 1
  assert result.stderr == "mknod: missing operand after '1'\nTry 'mknod --help' for more information.\n", result.stderr
}

type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "", diagnostics: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mkfifo.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C", UUTILS_DIAG: diagnostics}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_mkfifo_symbolic_modes_and_continue_after_error { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo")?
  let result = run_applet(ctx, root, ["-m", "u=rw,g=r,o=", "pipe", "pipe", "other"])?
  assert result.status == 1
  assert "File exists" in result.stderr
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o640
  assert fs.stat(fp"{root}/other")?.kind == "fifo"
}

test test_mkfifo_invalid_modes_do_not_create { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-invalid")?
  assert run_applet(ctx, root, ["-m", "1777", "pipe"])?.status == 1
  assert ! fp"{root}/pipe".exists()?
  assert run_applet(ctx, root, ["-m", "u=invalid", "pipe"])?.status == 1
}

test test_mkfifo_invalid_mode_diagnostic_marks_the_bad_operator { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-mode-diagnostic")?
  let result = run_applet(ctx, root, ["-m", "+rw?", "some_pipe"], diagnostics: "always")?
  assert result.status == 1
  assert "mkfifo:1:7" in result.stderr, result.stderr
  assert " 1 │ -m +rw? some_pipe" in result.stderr, result.stderr
}


test test_mkfifo_copied_permissions_and_multiple_mode_operations { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-mode-copy")?
  let result = run_applet(ctx, root, ["-m", "u=rw-x,g=u-w,o=", "pipe"])?
  assert result.status == 0, result.stderr
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o640
}


test test_mkfifo_unspecified_classes_obey_umask { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-umask")?
  assert run_applet(ctx, root, ["-m", "=rwx", "pipe"])?.status == 0
  assert fs.stat(fp"{root}/pipe")?.mode.bit_and(0o777) == 0o777.clear_bits(fs.umask()?)
}

test test_mkfifo_bare_context_flags_are_silent { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-bare-context")?
  for args in [["-Z", "pipe"], ["--context", "pipe-long"], ["--con", "pipe-abbrev"]] {
    let result = run_applet(ctx, root, args)?
    assert result.status == 0, result.stderr
    assert result.stderr == "", result.stderr
    assert fs.stat(fp"{root}/{args[1]}")?.kind == "fifo"
  }
  let moded = run_applet(ctx, root, ["-Zm600", "moded"])?
  assert moded.status == 0, moded.stderr
  assert moded.stderr == ""
  assert fs.stat(fp"{root}/moded")?.mode.bit_and(0o777) == 0o600
}

test test_mkfifo_context_value_warns_and_still_creates { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-context-value")?
  let result = run_applet(ctx, root, ["--context=unconfined_u:object_r:user_tmp_t:s0", "pipe"])?
  assert result.status == 0, result.stderr
  assert result.stderr == "mkfifo: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n", result.stderr
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
}

test test_mkfifo_context_value_warns_for_each_occurrence { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-context-repeated")?
  let warning = "mkfifo: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  let result = run_applet(ctx, root, ["--context=a", "--context=", "-Z", "--cont=b", "--context", "pipe"])?
  assert result.status == 0, result.stderr
  assert result.stderr == warning + warning + warning, result.stderr
  assert fs.stat(fp"{root}/pipe")?.kind == "fifo"
}

test test_mkfifo_context_warning_precedes_later_diagnostics { |ctx|
  let root = test.temp_dir(ctx, name: "mkfifo-context-diagnostics")?
  let warning = "mkfifo: warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel\n"
  let invalid = run_applet(ctx, root, ["--context=a", "-m", "999", "pipe"])?
  assert invalid.status == 1
  assert invalid.stderr.starts_with(warning), invalid.stderr
  assert ! fp"{root}/pipe".exists()?

  let missing = run_applet(ctx, root, ["--context=a"])?
  assert missing.status == 1
  assert missing.stderr == warning + "mkfifo: missing operand\nTry 'mkfifo --help' for more information.\n", missing.stderr
}

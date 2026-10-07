type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mktemp.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

test test_mktemp_templates_suffix_and_private_modes { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp")?
  let file = run_applet(ctx, root, ["--suffix=.txt", "file.XXXXXX"])?
  assert file.status == 0, file.stderr
  let name = file.stdout.trim()
  assert name.starts_with("file.") and name.ends_with(".txt")
  assert fs.stat(fp"{root}/{name}")?.mode.bit_and(0o777) == 0o600.clear_bits(fs.umask()?)
  let dir = run_applet(ctx, root, ["-d", "dir.XXXXXX"])?
  assert dir.status == 0, dir.stderr
  assert fs.stat(fp"{root}/{dir.stdout.trim()}")?.kind == "dir"
  assert fs.stat(fp"{root}/{dir.stdout.trim()}")?.mode.bit_and(0o777) == 0o700.clear_bits(fs.umask()?)
}

test test_mktemp_dry_run_and_tmpdir { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-dry")?
  let result = run_applet(ctx, root, ["-u", "--tmpdir", "tmp.XXXX"], tempdir: root.display())?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with(root.display() + "/tmp.")
  assert ! fp"{result.stdout.trim()}".exists()?
  assert run_applet(ctx, root, ["bad.XX"])?.status == 1
}

test test_mktemp_missing_tmpdir_value_reports_required_value { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-missing-tmpdir")?
  let result = run_applet(ctx, root, ["-p"])?
  assert result.status == 1
  assert "a value is required for '-p <DIR>' but none was supplied" in result.stderr, result.stderr
  assert result.stdout == ""
}

test test_mktemp_option_terminator_keeps_dash_p_as_template { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-dash-p-template")?
  let result = run_applet(ctx, root, ["--", "-p"])?
  assert result.status == 1
  assert "too few X's in template '-p'" in result.stderr, result.stderr
}


test test_mktemp_tmpdir_aliases_obey_command_line_order { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-option-order")?
  let last_long = run_applet(ctx, root, ["-u", "-p", "missing", "--tmpdir", "file.XXXX"], tempdir: root.display())?
  assert last_long.status == 0, last_long.stderr
  assert last_long.stdout.starts_with(root.display() + "/file.")
  let last_short = run_applet(ctx, root, ["-u", "--tmpdir", "file.XXXX", "-p", "."])?
  assert last_short.status == 0
  assert last_short.stdout.starts_with("./file.")
}

# Command file sinks persist captured output; descriptor redirection keeps the
# special device intact and makes the child observe the write failure itself.
test test_mktemp_failed_stdout_removes_created_entry { |ctx|
  guard p"/dev/full".exists()? else { test.skip("requires the full device"); return }
  for directory in [false, true] {
    let root = test.temp_dir(ctx, name: "mktemp-write-failure")?
    let script = fp"{ctx.core_dir}/mktemp.xsh"
    let args = ["sh", "-c", "exec \"$@\" > /dev/full", "mktemp-stdout", ctx.xsh_bin.display(), script.display(), "--"].extend(if directory { ["-d", "name.XXXXXX"] } else { ["name.XXXXXX"] })
    let err = fp"{root}/stderr"
    let plan = process.command_argv(p"/bin/sh", args, root, {LC_ALL: "C"}, b"", fp"{root}/stdout", err)
    assert process.run(plan)?.exit_code()? == 1
    assert "write error" in err.read_text()?
    for entry in fs.children(root) { assert ! entry.name.starts_with("name.") }
  }
}

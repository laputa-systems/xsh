type Outcome = {status: Int, stdout: Str, stderr: Str}

proc run_applet(ctx: TestContext, root: Path, args: List[Str], tempdir: Str = "") [fs, process, error] -> Result[Outcome] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mktemp.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {TMPDIR: tempdir, LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?})
}

type ByteOutcome = {status: Int, stdout: Bytes, stderr: Str}

proc run_applet_paths(ctx: TestContext, root: Path, args: List[Union[Str, Path]]) [fs, process, error] -> Result[ByteOutcome] {
  let out = fp"{root}/stdout-bytes"
  let err = fp"{root}/stderr-bytes"
  let script = fp"{ctx.core_dir}/mktemp.xsh"
  let words: List[Union[Str, Path]] = collect { yield ctx.xsh_bin; yield script; yield "--"; for arg in args { yield arg } }
  let plan = process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", TMPDIR: ""}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
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

test test_mktemp_non_utf8_template_is_preserved { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-template-bytes")?
  let pattern = Path.parse_bytes(b"template_\xff\xfe_XXXXXX")?
  let args: List[Union[Str, Path]] = [pattern]
  let result = run_applet_paths(ctx, root, args)?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with(b"template_\xff\xfe_")
  assert result.stdout.ends_with(b"\n")
  let created = Path.parse_bytes(bytes.concat([root.bytes(), b"/", result.stdout[0..result.stdout.len() - 1]]))?
  assert created.exists()?
}

test test_mktemp_non_utf8_tmpdir_paths { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-tmpdir-bytes")?
  let directory = Path.parse_bytes(bytes.concat([root.bytes(), b"/dir_\xff\xfe"]))?
  directory.mkdir(parents: false)?
  let short_args: List[Union[Str, Path]] = ["-p", directory]
  let short = run_applet_paths(ctx, root, short_args)?
  assert short.status == 0, short.stderr
  assert short.stdout.starts_with(bytes.concat([directory.bytes(), b"/tmp."]))
  let created_short = Path.parse_bytes(short.stdout[0..short.stdout.len() - 1])?
  assert created_short.exists()?
  let tmpdir_option = Path.parse_bytes(bytes.concat([b"--tmpdir=", directory.bytes()]))?
  let long_args: List[Union[Str, Path]] = [tmpdir_option, "tmpXXXXXX"]
  let long = run_applet_paths(ctx, root, long_args)?
  assert long.status == 0, long.stderr
  assert long.stdout.starts_with(bytes.concat([directory.bytes(), b"/tmp"]))
  let created_long = Path.parse_bytes(long.stdout[0..long.stdout.len() - 1])?
  assert created_long.exists()?
}

test test_mktemp_non_utf8_suffix { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-suffix-bytes")?
  let suffix = Path.parse_bytes(b"\xc3|\xed\xba\xad")?
  let args: List[Union[Str, Path]] = ["-p", root, "--suffix", suffix, "tmpXXXXXX"]
  let result = run_applet_paths(ctx, root, args)?
  assert result.status == 0, result.stderr
  assert result.stdout.ends_with(bytes.concat([suffix.bytes(), b"\n"]))
  let created = Path.parse_bytes(result.stdout[0..result.stdout.len() - 1])?
  assert created.exists()?
}

test test_mktemp_creates_directory_under_non_utf8_tmpdir { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-dir-bytes")?
  let parent = Path.parse_bytes(bytes.concat([root.bytes(), b"/parent_\xff\xfe"]))?
  parent.mkdir(parents: false)?
  let args: List[Union[Str, Path]] = ["-d", "-p", parent]
  let result = run_applet_paths(ctx, root, args)?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with(bytes.concat([parent.bytes(), b"/tmp."]))
  let created = Path.parse_bytes(result.stdout[0..result.stdout.len() - 1])?
  assert fs.stat(created)?.kind == "dir"
}


test test_mktemp_tmpdir_aliases_obey_command_line_order { |ctx|
  let root = test.temp_dir(ctx, name: "mktemp-option-order")?
  let last_long = run_applet(ctx, root, ["-u", "-p", "missing", "--tmpdir", "file.XXXX"], tempdir: root.display())?
  assert last_long.status == 0, last_long.stderr
  assert last_long.stdout.starts_with(root.display() + "/file.")
  let last_short = run_applet(ctx, root, ["-u", "--tmpdir", "file.XXXX", "-p", "."])?
  assert last_short.status == 0
  assert last_short.stdout.starts_with("./file.")
  let legacy_parent_args: List[Union[Str, Path]] = ["-t", "-p", root, "legacy.XXXX"]
  let legacy_parent = run_applet_paths(ctx, root, legacy_parent_args)?
  assert legacy_parent.status == 0, legacy_parent.stderr
  assert legacy_parent.stdout.starts_with(bytes.concat([root.bytes(), b"/legacy."]))
  assert Path.parse_bytes(legacy_parent.stdout[0..legacy_parent.stdout.len() - 1])?.exists()?
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

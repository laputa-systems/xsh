type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Capture the real applet and its stdin without involving an external checksum.
proc invoke(ctx: TestContext, args: List[Str], input: Bytes) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "md5sum-run")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/md5sum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--"].extend(args),
    root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_md5sum_stdin_known_vector { |ctx|
  let result = invoke(ctx, [], b"abc")?
  assert result.status == 0
  assert result.stdout == b"900150983cd24fb0d6963f7d28e17f72  -\n"
  assert result.stderr == ""
}

test test_md5sum_continues_after_unreadable_file { |ctx|
  let file = test.temp_file(ctx, name: "md5sum-data", contents: b"abc")?
  let missing = test.temp_path(ctx, name: "md5sum-missing")
  let result = invoke(ctx, [missing.display(), file.display()], b"")?
  assert result.status == 1
  assert result.stdout.len() > 0
  assert result.stderr.find("No such file or directory") != null
}

test test_md5sum_check_and_strict_status { |ctx|
  let file = test.temp_file(ctx, name: "md5sum-verify", contents: b"abc")?
  let list = test.temp_file(ctx, name: "md5sum-list", contents: bytes.from_text(f"900150983cd24fb0d6963f7d28e17f72  {file}\n"))?
  let checked = invoke(ctx, ["-c", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n")
  let quiet = invoke(ctx, ["-c", "--quiet", list.display()], b"")?
  assert quiet.status == 0
  assert quiet.stdout == b""
  let malformed = test.temp_file(ctx, name: "md5sum-bad-list", contents: bytes.from_text(f"junk\n900150983cd24fb0d6963f7d28e17f72  {file}\n"))?
  let strict = invoke(ctx, ["-c", "--strict", "--status", malformed.display()], b"")?
  assert strict.status == 1
  assert strict.stdout == b""
}

test test_md5sum_zero_binary_and_tagged { |ctx|
  let binary = invoke(ctx, ["-bz"], b"abc")?
  assert binary.status == 0
  assert binary.stdout == b"900150983cd24fb0d6963f7d28e17f72 *-\0"
  let tagged = invoke(ctx, ["--tag"], b"abc")?
  assert tagged.status == 0
  assert tagged.stdout == b"MD5 (-) = 900150983cd24fb0d6963f7d28e17f72\n"
}

test test_md5sum_escaped_unicode_file_roundtrip { |ctx|
  let directory = test.temp_dir(ctx, name: "checksum-escaped")?
  let file = fp"{directory}/checksum-µ\\file"
  file.write("abc")
  let emitted = invoke(ctx, [file.display()], b"")?
  assert emitted.status == 0
  assert emitted.stdout.utf8()?.starts_with("\\900150983cd24fb0d6963f7d28e17f72  "), f"path={file}, output={emitted.stdout.utf8()?}"
  let list = test.temp_file(ctx, name: "escaped-check-list", contents: emitted.stdout)?
  let checked = invoke(ctx, ["--check", list.display()], b"")?
  assert checked.status == 0
  assert checked.stderr == ""
}

test test_md5sum_check_quotes_unusual_filenames { |ctx|
  let directory = test.temp_dir(ctx, name: "checksum-quoted")?
  let spaced = fp"{directory}/ leading"
  let starred = fp"{directory}/*literal"
  spaced.write("")
  starred.write("")
  let list = test.temp_file(ctx, name: "quoted-check-list", contents: bytes.from_text(
    f"d41d8cd98f00b204e9800998ecf8427e  {spaced}\nd41d8cd98f00b204e9800998ecf8427e  {starred}\n"))?

  let checked = invoke(ctx, ["--check", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"'{spaced}': OK\n'{starred}': OK\n")
  assert checked.stderr == ""
}

test test_md5sum_check_shell_quotes_control_filenames { |ctx|
  let directory = test.temp_dir(ctx, name: "checksum-control-name")?
  let file = fp"{directory}/line\nbreak"
  file.write("")
  let emitted = invoke(ctx, [file.display()], b"")?
  assert emitted.status == 0
  let list = test.temp_file(ctx, name: "control-check-list", contents: emitted.stdout)?

  let checked = invoke(ctx, ["--check", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"'{directory}/line'$'\\n''break': OK\n")
  assert checked.stderr == ""
}

test test_md5sum_check_ignores_blank_lines { |ctx|
  let file = test.temp_file(ctx, name: "blank-line-check", contents: b"")?
  let list = test.temp_file(ctx, name: "blank-line-list", contents: bytes.from_text(
    f"\nd41d8cd98f00b204e9800998ecf8427e  {file}\n\ninvalid\n"))?

  let checked = invoke(ctx, ["--check", "--warn", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n")
  assert checked.stderr.find(": 4: improperly formatted MD5 checksum line") != null
  assert checked.stderr.find("WARNING: 1 line is improperly formatted") != null
}

test test_md5sum_compact_tag_separator { |ctx|
  let directory = test.temp_dir(ctx, name: "compact-tag")?
  let file = fp"{directory}/f"
  file.write("")
  let list = test.temp_file(ctx, name: "compact-tag-list", contents: bytes.from_text(
    f"MD5({file})= d41d8cd98f00b204e9800998ecf8427e\n"))?

  let checked = invoke(ctx, ["--check", list.display()], b"")?
  assert checked.status == 0
  assert checked.stdout == bytes.from_text(f"{file}: OK\n")
  assert checked.stderr == ""
}

test test_md5sum_check_preserves_leading_space_filename { |ctx|
  let root = test.temp_dir(ctx, name: "leading-space-check")?
  let file = fp"{root}/ b"
  file.write(" b\n")
  let list = fp"{root}/check.md5sum"
  list.write("bf35d7536c785cf06730d5a40301eba2  b\n")
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/md5sum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--", "--strict", "--check", list.display()],
    root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  assert status.exit_code()? == 0
  assert out.read_bytes()? == b"' b': OK\n"
  assert err.read_text()? == ""
}

test test_md5sum_check_accepts_bsd_alternate_format { |ctx|
  let root = test.temp_dir(ctx, name: "alternate-format-check")?
  for name in ["a", " b", "*c"] {
    fp"{root}/{name}".write("")
  }
  let list = fp"{root}/check.md5sum"
  list.write("d41d8cd98f00b204e9800998ecf8427e a\nd41d8cd98f00b204e9800998ecf8427e  b\nd41d8cd98f00b204e9800998ecf8427e *c\n")
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/md5sum.xsh"
  let plan = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "--", "--strict", "--check", list.display()],
    root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  assert status.exit_code()? == 0
  assert out.read_bytes()? == b"a: OK\n' b': OK\n'*c': OK\n"
  assert err.read_text()? == ""
}

test test_md5sum_invalid_prefix_does_not_select_list_format { |ctx|
  let file = test.temp_file(ctx, name: "format-prefix-file", contents: b"")?
  let list = test.temp_file(ctx, name: "format-prefix-list", contents: bytes.from_text(
    f"not-a-digest garbage\nd41d8cd98f00b204e9800998ecf8427e  {file}\n"))?
  let result = invoke(ctx, ["--check", "--warn", list.display()], b"")?
  assert result.status == 0
  assert result.stdout == bytes.from_text(f"{file}: OK\n")
  assert result.stderr.find(f"{list}: 1: improperly formatted MD5 checksum line") != null
  assert result.stderr.find("WARNING: 1 line is improperly formatted") != null
}

test test_md5sum_invalid_option_uses_gnu_exit_status { |ctx|
  let result = invoke(ctx, ["--definitely-invalid"], b"")?
  assert result.status == 1
  assert result.stderr.find("unrecognized option") != null
}

test test_md5sum_invalid_modes_omit_help_hint { |ctx|
  let tagged_check = invoke(ctx, ["--tag", "--check", "/dev/null"], b"")?
  assert tagged_check.status == 1
  assert tagged_check.stderr == "md5sum: the --tag option is meaningless when verifying checksums\n"

  let tagged_text = invoke(ctx, ["--tag", "--text"], b"")?
  assert tagged_text.status == 1
  assert tagged_text.stderr == "md5sum: --tag does not support --text mode\n"

  let ignore_missing = invoke(ctx, ["--ignore-missing"], b"")?
  assert ignore_missing.status == 1
  assert ignore_missing.stderr == "md5sum: the --ignore-missing option is meaningful only when verifying checksums\nTry 'md5sum --help' for more information.\n"
}

test test_md5sum_missing_check_path_keeps_original_name { |ctx|
  let list = test.temp_file(ctx, name: "missing-check-path", contents: bytes.from_text(
    "d41d8cd98f00b204e9800998ecf8427e  missing\n"))?
  let checked = invoke(ctx, ["--check", list.display()], b"")?
  assert checked.status == 1
  assert checked.stdout == b"missing: FAILED open or read\n"
  assert checked.stderr.find("missing: No such file or directory") != null
}

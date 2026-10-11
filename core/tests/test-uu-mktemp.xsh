##! Transcribed from the uutils coreutils mktemp integration suite.
use support.uu as uu

# Only X characters are wildcards; every other byte and the length must match.
proc matches_template(pattern: Str, filename: Str) {
  assert pattern.byte_len() == filename.byte_len(), f"{filename} does not match {pattern}"
  for i in range(pattern.byte_len()) {
    assert pattern.byte_slice(i, length: 1) == "X" or pattern.byte_slice(i, length: 1) == filename.byte_slice(i, length: 1), f"{filename} does not match {pattern}"
  }
}

proc output_name(r: uu.Ran) [error] -> Result[Str] {
  let name = r.stdout.utf8()?
  var end = name.byte_len()
  while end > 0 and name.byte_slice(end - 1, length: 1).trim().is_empty() { end -= 1 }
  Ok(name.byte_slice(0, length: end))
}

proc created(s: uu.Scene, r: uu.Ran, pattern: Str, directory: Bool = false) [fs, error] {
  uu.succeeds(r)
  uu.no_stderr(r)
  let name = output_name(r)?
  matches_template(pattern, name)
  let target = if name.starts_with("/") { Path(name) } else { uu.at(s, name) }
  if directory { assert target.is_dir()? } else { assert target.is_file()? }
}

# origin: uutils test_mktemp::test_mktemp_mktemp
test test_uu_mktemp_mktemp_mktemp { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["tempXXXXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "mktemp", ["temp"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r1)
  let r2 = uu.invoke(s, "mktemp", ["tempX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r2)
  let r3 = uu.invoke(s, "mktemp", ["tempXX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r3)
  let r4 = uu.invoke(s, "mktemp", ["tempXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "mktemp", ["tempXXXlate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "mktemp", ["XXXtemplate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "mktemp", ["tempXXXl/ate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r7)
  let r8 = uu.invoke(s, "mktemp", ["XXX_XX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r8)
}

# origin: uutils test_mktemp::test_mktemp_mktemp_t
test test_uu_mktemp_mktemp_mktemp_t { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["-t", "tempXXXXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "mktemp", ["-t", "temp"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r1)
  let r2 = uu.invoke(s, "mktemp", ["-t", "tempX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r2)
  let r3 = uu.invoke(s, "mktemp", ["-t", "tempXX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r3)
  let r4 = uu.invoke(s, "mktemp", ["-t", "tempXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "mktemp", ["-t", "tempXXXlate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "mktemp", ["-t", "XXXtemplate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "mktemp", ["-t", "tempXXXl/ate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r7)
  uu.no_stdout(r7)
  uu.stderr_contains(r7, "invalid suffix")
  uu.stderr_contains(r7, "contains directory separator")
  let r8 = uu.invoke(s, "mktemp", ["-t", "XXX_XX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r8)
}

# origin: uutils test_mktemp::test_mktemp_make_temp_dir
test test_uu_mktemp_mktemp_make_temp_dir { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["-d", "tempXXXXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "mktemp", ["-d", "temp"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r1)
  let r2 = uu.invoke(s, "mktemp", ["-d", "tempX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r2)
  let r3 = uu.invoke(s, "mktemp", ["-d", "tempXX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r3)
  let r4 = uu.invoke(s, "mktemp", ["-d", "tempXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "mktemp", ["-d", "tempXXXlate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "mktemp", ["-d", "XXXtemplate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "mktemp", ["-d", "tempXXXl/ate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r7)
  let r8 = uu.invoke(s, "mktemp", ["-d", "XXX_XX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r8)
}

# origin: uutils test_mktemp::test_mktemp_dry_run
test test_uu_mktemp_mktemp_dry_run { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["-u", "tempXXXXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "mktemp", ["-u", "temp"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r1)
  let r2 = uu.invoke(s, "mktemp", ["-u", "tempX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r2)
  let r3 = uu.invoke(s, "mktemp", ["-u", "tempXX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r3)
  let r4 = uu.invoke(s, "mktemp", ["-u", "tempXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "mktemp", ["-u", "tempXXXlate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "mktemp", ["-u", "XXXtemplate"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "mktemp", ["-u", "tempXXXl/ate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r7)
  let r8 = uu.invoke(s, "mktemp", ["-u", "XXX_XX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r8)
}

# origin: uutils test_mktemp::test_mktemp_suffix
test test_uu_mktemp_mktemp_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["--suffix", "suf", "tempXXXXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "mktemp", ["--suffix", "suf", "temp"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r1)
  let r2 = uu.invoke(s, "mktemp", ["--suffix", "suf", "tempX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r2)
  let r3 = uu.invoke(s, "mktemp", ["--suffix", "suf", "tempXX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r3)
  let r4 = uu.invoke(s, "mktemp", ["--suffix", "suf", "tempXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "mktemp", ["--suffix", "suf", "tempXXXlate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r5)
  let r6 = uu.invoke(s, "mktemp", ["--suffix", "suf", "XXXtemplate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r6)
  let r7 = uu.invoke(s, "mktemp", ["--suffix", "suf", "tempXXXl/ate"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r7)
  let r8 = uu.invoke(s, "mktemp", ["--suffix", "suf", "XXX_XX"], vars: {TMPDIR: s.root.display()})?
  uu.fails(r8)
}

# origin: uutils test_mktemp::test_mktemp_tmpdir
test test_uu_mktemp_mktemp_tmpdir { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "tempXXXXXX"])?
  uu.succeeds(r0)
  let r1 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "temp"])?
  uu.fails(r1)
  let r2 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "tempX"])?
  uu.fails(r2)
  let r3 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "tempXX"])?
  uu.fails(r3)
  let r4 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "tempXXX"])?
  uu.succeeds(r4)
  let r5 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "tempXXXlate"])?
  uu.succeeds(r5)
  let r6 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "XXXtemplate"])?
  uu.succeeds(r6)
  let r7 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "tempXXXl/ate"])?
  uu.fails(r7)
  let r8 = uu.invoke(s, "mktemp", ["-p", s.root.display(), "XXX_XX"])?
  uu.fails(r8)
}

# origin: uutils test_mktemp::test_mktemp_tmpdir_one_arg
test test_uu_mktemp_mktemp_tmpdir_one_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["--tmpdir", "apt-key-gpghome.XXXXXXXXXX"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "apt-key-gpghome.")
  let target = Path(output_name(r)?)
  assert target.is_file()?
  target.remove()?
}

# origin: uutils test_mktemp::test_mktemp_directory_tmpdir
test test_uu_mktemp_mktemp_directory_tmpdir { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["--directory", "--tmpdir", "apt-key-gpghome.XXXXXXXXXX"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_contains(r, "apt-key-gpghome.")
  let target = Path(output_name(r)?)
  assert target.is_dir()?
  target.remove()?
}

# origin: uutils test_mktemp::test_mktemp_quiet
test test_uu_mktemp_mktemp_quiet { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-p", "/definitely/not/exist/I/promise", "-q"], ["-d", "-p", "/definitely/not/exist/I/promise", "-q"]] {
    let r = uu.invoke(s, "mktemp", args)?
    uu.fails(r)
    uu.no_output(r)
  }
}

# origin: uutils test_mktemp::test_mktemp_empty_tmpdir
test test_uu_mktemp_mktemp_empty_tmpdir { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-p", ""], ["--tmpdir="]] {
    let r = uu.invoke(s, "mktemp", args, vars: {TMPDIR: s.root.display()})?
    uu.succeeds(r)
    assert output_name(r)?.starts_with(s.root.display())
  }
}

# origin: uutils test_mktemp::test_respect_template
test test_uu_mktemp_respect_template { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["XXX"])?
  created(s, r, "XXX", directory: false)
}

# origin: uutils test_mktemp::test_respect_template_directory
test test_uu_mktemp_respect_template_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "d")?
  let r = uu.invoke(s, "mktemp", ["d/XXX"])?
  created(s, r, "d/XXX", directory: false)
}

# origin: uutils test_mktemp::test_directory_permissions
test test_uu_mktemp_directory_permissions { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-d", "XXX"])?
  created(s, r, "XXX", directory: true)
  assert fs.stat(uu.at(s, output_name(r)?))?.mode == 0o40700
}

# origin: uutils test_mktemp::test_tmpdir_template_has_subdirectory
test test_uu_mktemp_tmpdir_template_has_subdirectory { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a")?
  let r = uu.invoke(s, "mktemp", ["--tmpdir=.", "a/bXXXX"])?
  created(s, r, "./a/bXXXX", directory: false)
}

# origin: uutils test_mktemp::test_two_contiguous_wildcard_blocks
test test_uu_mktemp_two_contiguous_wildcard_blocks { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["XXX_XXX"])?
  created(s, r, "XXX_XXX", directory: false)
  assert output_name(r)?.starts_with("XXX_")
}

# origin: uutils test_mktemp::test_three_contiguous_wildcard_blocks
test test_uu_mktemp_three_contiguous_wildcard_blocks { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["XXX_XXX_XXX"])?
  created(s, r, "XXX_XXX_XXX", directory: false)
  assert output_name(r)?.starts_with("XXX_XXX_")
}

# origin: uutils test_mktemp::test_tmpdir_absolute_path
test test_uu_mktemp_tmpdir_absolute_path { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["--tmpdir=a", "/XXX"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: invalid template, '/XXX'; with --tmpdir, it may not be absolute\n")
}

# origin: uutils test_mktemp::test_template_path_separator
test test_uu_mktemp_template_path_separator { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-t", "a/bXXX"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: invalid template, 'a/bXXX', contains directory separator\n")
}

# origin: uutils test_mktemp::test_prefix_template_with_path_separator
test test_uu_mktemp_prefix_template_with_path_separator { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-t", "a/XXX"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: invalid template, 'a/XXX', contains directory separator\n")
}

# origin: uutils test_mktemp::test_too_few_xs_suffix
test test_uu_mktemp_too_few_xs_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["--suffix=X", "aXX"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: too few X's in template 'aXX'\n")
}

# origin: uutils test_mktemp::test_too_few_xs_suffix_directory
test test_uu_mktemp_too_few_xs_suffix_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-d", "--suffix=X", "aXX"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: too few X's in template 'aXX'\n")
}

# origin: uutils test_mktemp::test_too_few_xs_quiet
test test_uu_mktemp_too_few_xs_quiet { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-q", "aXX"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: too few X's in template 'aXX'\n")
}

# origin: uutils test_mktemp::test_suffix_must_end_in_x
test test_uu_mktemp_suffix_must_end_in_x { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["--suffix=", "aXXXb"])?
  uu.fails(r)
  uu.stderr_is(r, "mktemp: with --suffix, template 'aXXXb' must end in X\n")
}

# origin: uutils test_mktemp::test_suffix_path_separator
test test_uu_mktemp_suffix_path_separator { |ctx|
  let s = uu.scene(ctx)?
  for item in [{template: "aXXX/b", suffix: "/b"}, {template: "XXX/..", suffix: "/.."}] {
    let r = uu.invoke(s, "mktemp", [item.template])?
    uu.fails(r)
    uu.stderr_only(r, f"mktemp: invalid suffix '{item.suffix}', contains directory separator\n")
  }
}

# origin: uutils test_mktemp::test_suffix_empty_template
test test_uu_mktemp_suffix_empty_template { |ctx|
  let s = uu.scene(ctx)?
  for args in [["--suffix=aXXXb", ""], ["-d", "--suffix=aXXXb", ""]] {
    let r = uu.invoke(s, "mktemp", args)?
    uu.fails(r)
    uu.stderr_is(r, "mktemp: with --suffix, template '' must end in X\n")
  }
}

# origin: uutils test_mktemp::test_too_many_arguments
test test_uu_mktemp_too_many_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-q", "a", "b"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "mktemp: too many templates\nTry 'mktemp --help' for more information.\n")
}

# origin: uutils test_mktemp::test_mktemp_with_posixly_correct
test test_uu_mktemp_mktemp_with_posixly_correct { |ctx|
  let s = uu.scene(ctx)?
  let bad = uu.invoke(s, "mktemp", ["aXXXX", "--suffix=b"], vars: {POSIXLY_CORRECT: "1"})?
  uu.fails(bad)
  uu.stderr_only(bad, "mktemp: too many templates\nTry 'mktemp --help' for more information.\n")
  uu.succeeds(uu.invoke(s, "mktemp", ["--suffix=b", "aXXXX"], vars: {POSIXLY_CORRECT: "1"})?)
}

# origin: uutils test_mktemp::test_tmpdir_env_var
test test_uu_mktemp_tmpdir_env_var { |ctx|
  let s = uu.scene(ctx)?
  for item in [{args: [], template: "./tmp.XXXXXXXXXX"}, {args: ["--tmpdir"], template: "./tmp.XXXXXXXXXX"}, {args: ["--tmpdir", "XXX"], template: "./XXX"}, {args: ["XXX"], template: "XXX"}] {
    created(s, uu.invoke(s, "mktemp", item.args, vars: {TMPDIR: "."})?, item.template)
  }
}

# origin: uutils test_mktemp::test_nonexistent_tmpdir_env_var
test test_uu_mktemp_nonexistent_tmpdir_env_var { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", [], vars: {TMPDIR: "no/such/dir"})?
  uu.fails(r0)
  uu.stderr_only(r0, "mktemp: failed to create file via template 'no/such/dir/tmp.XXXXXXXXXX': No such file or directory\n")
  let r1 = uu.invoke(s, "mktemp", ["-d"], vars: {TMPDIR: "no/such/dir"})?
  uu.fails(r1)
  uu.stderr_only(r1, "mktemp: failed to create directory via template 'no/such/dir/tmp.XXXXXXXXXX': No such file or directory\n")
}

# origin: uutils test_mktemp::test_nonexistent_dir_prefix
test test_uu_mktemp_nonexistent_dir_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "mktemp", ["d/XXX"])?
  uu.fails(r0)
  uu.stderr_only(r0, "mktemp: failed to create file via template 'd/XXX': No such file or directory\n")
  let r1 = uu.invoke(s, "mktemp", ["-d", "d/XXX"])?
  uu.fails(r1)
  uu.stderr_only(r1, "mktemp: failed to create directory via template 'd/XXX': No such file or directory\n")
}

# origin: uutils test_mktemp::test_empty_tmpdir_env_var
test test_uu_mktemp_empty_tmpdir_env_var { |ctx|
  let s = uu.scene(ctx)?
  for args in [[], ["-d"]] {
    let r = uu.invoke(s, "mktemp", args, vars: {TMPDIR: ""})?
    uu.succeeds(r)
    uu.stdout_str_starts_with(r, "/tmp")
    Path(output_name(r)?).remove()?
  }
}

# origin: uutils test_mktemp::test_default_missing_value
test test_uu_mktemp_default_missing_value { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-d", "--tmpdir"])?
  uu.succeeds(r)
  Path(output_name(r)?).remove()?
}

# origin: uutils test_mktemp::test_default_issue_4821_t_tmpdir
test test_uu_mktemp_default_issue_4821_t_tmpdir { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-t", "foo.XXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r)
  uu.stdout_contains(r, s.root.display())
}

# origin: uutils test_mktemp::test_default_issue_4821_t_tmpdir_p
test test_uu_mktemp_default_issue_4821_t_tmpdir_p { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-t", "-p", s.root.display(), "foo.XXXX"])?
  uu.succeeds(r)
  uu.stdout_contains(r, s.root.display())
}

# origin: uutils test_mktemp::test_t_ensure_tmpdir_has_higher_priority_than_p
test test_uu_mktemp_t_ensure_tmpdir_has_higher_priority_than_p { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-t", "-p", "should_not_attempt_to_write_in_this_nonexisting_dir", "foo.XXXX"], vars: {TMPDIR: s.root.display()})?
  uu.succeeds(r)
  uu.stdout_contains(r, s.root.display())
}

# origin: uutils test_mktemp::test_prefix_template_separator
test test_uu_mktemp_prefix_template_separator { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "mktemp", ["-p", ".", "-t", "a.XXXX"])?)
}

# origin: uutils test_mktemp::test_missing_xs_tmpdir_template
test test_uu_mktemp_missing_xs_tmpdir_template { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["--tmpdir", "tempX"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "too few X's in template")
  let r2 = uu.invoke(s, "mktemp", ["--tmpdir=foobar"])?
  uu.fails(r2)
  uu.no_stdout(r2)
  uu.stderr_contains(r2, "failed to create file via template")
}

# origin: uutils test_mktemp::test_both_tmpdir_flags_present
test test_uu_mktemp_both_tmpdir_flags_present { |ctx|
  let s = uu.scene(ctx)?
  created(s, uu.invoke(s, "mktemp", ["-p", "nonsense", "--tmpdir", "foobarXXXX"], vars: {TMPDIR: "."})?, "./foobarXXXX")
  let bad = uu.invoke(s, "mktemp", ["-p", ".", "--tmpdir=does_not_exist"])?
  uu.fails(bad)
  uu.no_stdout(bad)
  uu.stderr_contains(bad, "failed to create file via template")
  created(s, uu.invoke(s, "mktemp", ["--tmpdir", "foobarXXXX", "-p", "."])?, "./foobarXXXX")
}

# origin: uutils test_mktemp::test_missing_short_tmpdir_flag
test test_uu_mktemp_missing_short_tmpdir_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-p"])?
  uu.fails(r)
  uu.stderr_only(r, "mktemp: option requires an argument -- 'p'\nTry 'mktemp --help' for more information.\n")
}

# origin: uutils test_mktemp::test_non_utf8_tmpdir_path
test test_uu_mktemp_non_utf8_tmpdir_path { |ctx|
  let s = uu.scene(ctx)?
  let dir = uu.at_bytes(s, b"test_dir_\xff\xfe")?
  dir.mkdir()?
  uu.succeeds(uu.invoke_paths(s, "mktemp", [p"-p", dir])?)
}

# origin: uutils test_mktemp::test_non_utf8_tmpdir_long_option
test test_uu_mktemp_non_utf8_tmpdir_long_option { |ctx|
  let s = uu.scene(ctx)?
  let dir = uu.at_bytes(s, b"test_dir_\xff\xfe")?
  dir.mkdir()?
  uu.succeeds(uu.invoke_paths(s, "mktemp", [p"-p", dir, p"tmpXXXXXX"])?)
}

# origin: uutils test_mktemp::test_non_utf8_tmpdir_directory_creation
test test_uu_mktemp_non_utf8_tmpdir_directory_creation { |ctx|
  let s = uu.scene(ctx)?
  let dir = uu.at_bytes(s, b"test_dir_\xff\xfe")?
  dir.mkdir()?
  uu.succeeds(uu.invoke_paths(s, "mktemp", [p"-d", p"-p", dir])?)
}

# origin: uutils test_mktemp::test_invalid_utf8_suffix
test test_uu_mktemp_invalid_utf8_suffix { |ctx|
  let s = uu.scene(ctx)?
  let suffix = Path.parse_bytes(b"\xc3|\xed\xba\xad")?
  uu.succeeds(uu.invoke_paths(s, "mktemp", [p"-p", s.root, p"--suffix", suffix, p"tmpXXXXXX"])?)
}

# origin: uutils test_mktemp::test_mktemp_hidden_file_single_dot
test test_uu_mktemp_mktemp_hidden_file_single_dot { |ctx|
  let s = uu.scene(ctx)?
  let dir = test.temp_dir(ctx, name: "hidden")?
  let r = uu.invoke(s, "mktemp", [fp"{dir}/.XXXXXX".display()])?
  uu.succeeds(r)
  let components = Path(r.stdout.utf8()?.trim()).components()
  let filename = components[components.len() - 1].display()
  assert filename.starts_with(".")
  assert filename.byte_len() == 7
}

# origin: uutils test_mktemp::test_write_error_removes_the_temporary_file
test test_uu_mktemp_write_error_removes_the_temporary_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["keep-none-XXXX"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_is(r, "mktemp: write error: No space left on device\n")
  assert s.root.glob("keep-none-*")?.is_empty()
}

# origin: uutils test_mktemp::test_write_error_removes_the_temporary_directory
test test_uu_mktemp_write_error_removes_the_temporary_directory { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "mktemp", ["-d", "keep-none-XXXX"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_is(r, "mktemp: write error: No space left on device\n")
  assert s.root.glob("keep-none-*")?.is_empty()
}

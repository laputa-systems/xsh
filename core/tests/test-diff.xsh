type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "diff-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/diff.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_diff_unified_normal_and_equal_status { |ctx|
  let left = test.temp_file(ctx, name: "left", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "right", contents: b"a\nc\n")?
  let unified = invoke(ctx, ["-u", left.display(), right.display()])?
  assert unified.status == 1
  assert unified.stdout.find("@@ -1,2 +1,2 @@") != null
  assert unified.stdout.find("-b\n+c\n") != null
  let normal = invoke(ctx, [left.display(), right.display()])?
  assert normal.status == 1
  assert normal.stdout == "2c2\n< b\n---\n> c\n"
  assert invoke(ctx, [left.display(), left.display()])?.status == 0
}

test test_diff_stdin_and_brief { |ctx|
  let file = test.temp_file(ctx, name: "right", contents: b"second\n")?
  let result = invoke(ctx, ["-q", "-", file.display()], b"first\n")?
  assert result.status == 1
  assert result.stdout.find("differ") != null
}

test test_diff_insert_delete_labels_and_binary { |ctx|
  let empty = test.temp_file(ctx, name: "empty")?
  let one = test.temp_file(ctx, name: "one", contents: b"one\n")?
  assert invoke(ctx, [empty.display(), one.display()])?.stdout == "0a1\n> one\n"
  assert invoke(ctx, [one.display(), empty.display()])?.stdout == "1d0\n< one\n"
  let labeled = invoke(ctx, ["-u", "-L", "before", "-L", "after", empty.display(), one.display()])?
  assert labeled.stdout.starts_with("--- before\n+++ after\n")
  let binary_file = test.temp_file(ctx, name: "binary", contents: b"one\0")?
  assert invoke(ctx, [binary_file.display(), one.display()])?.stdout.starts_with("Binary files ")
}

test test_diff_missing_file_and_unsupported_modes_fail { |ctx|
  let present = test.temp_file(ctx, name: "present", contents: b"x")?
  let missing = test.temp_path(ctx, name: "missing")
  assert invoke(ctx, [present.display(), missing.display()])?.status == 2
  assert invoke(ctx, ["-y", present.display(), present.display()])?.status == 2
}

test test_diff_unified_short_context_takes_separate_value { |ctx|
  let left = test.temp_file(ctx, name: "context-left", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "context-right", contents: b"a\nc\n")?
  let result = invoke(ctx, ["-U", "0", left.display(), right.display()])?
  assert result.status == 1, result.stderr
  assert result.stdout.find("@@ -2 +2 @@") != null
}

test test_diff_ignore_space_change_compares_whitespace_runs { |ctx|
  let old = test.temp_file(ctx, name: "space-old", contents: b"a \t c\n")?
  let same = invoke(ctx, ["-ub", old.display(), "-"], b"a\t \tc\n")?
  assert same.status == 0, same.stderr
  assert same.stdout == ""

  let trailing = invoke(ctx, ["-ub", old.display(), "-"], b"a \t c   \n")?
  assert trailing.status == 0

  let indented = invoke(ctx, ["-b", old.display(), "-"], b" a \t c\n")?
  assert indented.status == 1, "leading whitespace is a change"
}

test test_diff_ignore_space_change_at_end_of_file { |ctx|
  let old = test.temp_file(ctx, name: "eof-old", contents: b"abc")?
  let result = invoke(ctx, ["-ub", old.display(), "-"], b"abc ")?
  assert result.status == 0, result.stderr
  assert result.stdout == ""
}

test test_diff_ignore_space_change_takes_context_from_old_file { |ctx|
  let input = test.temp_file(ctx, name: "context-new", contents: b"abc\na  c\ndef\n")?
  let result = invoke(ctx, ["-ub", "-", input.display()], b"a c\n")?
  assert result.status == 1, result.stderr
  assert result.stdout == "--- -\n+++ " + input.display() + "\n@@ -1 +1,3 @@\n+abc\n a c\n+def\n", result.stdout
}

test test_diff_ignore_blank_lines_drops_only_blank_changes { |ctx|
  let input = test.temp_file(ctx, name: "blank-new", contents: b"a\n")?
  let blank_only = invoke(ctx, ["-uB", "-", input.display()], b"\na\n\n")?
  assert blank_only.status == 0, blank_only.stderr
  assert blank_only.stdout == ""

  let mixed = invoke(ctx, ["-uB", "-", input.display()], b"\nb\n\n")?
  assert mixed.status == 1
  assert mixed.stdout == "--- -\n+++ " + input.display() + "\n@@ -1,3 +1 @@\n-\n-b\n-\n+a\n", mixed.stdout
}

test test_diff_ignore_blank_lines_brief_and_single_line { |ctx|
  let input = test.temp_file(ctx, name: "single-new", contents: b"1\n")?
  let blank = invoke(ctx, ["-qB", "-", input.display()], b"\n1\n")?
  assert blank.status == 0
  assert blank.stdout == ""

  let changed = test.temp_file(ctx, name: "single-changed", contents: b"0\n")?
  let text = invoke(ctx, ["-qB", "-", changed.display()], b"1\n")?
  assert text.status == 1
  assert text.stdout == "Files - and " + changed.display() + " differ\n", text.stdout
}

# Two trees whose differences exercise every directory-comparison report.
# Names sort in byte order: bin, fd, ff, only, same, sub (and ronly in b).
proc build_trees(ctx: TestContext, name: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: name)?
  fp"{root}/a/sub".mkdir(parents: true)?
  fp"{root}/b/sub".mkdir(parents: true)?
  fp"{root}/a/same".mkdir()?
  fp"{root}/b/same".mkdir()?
  fp"{root}/a/same/q".write("same\n")?
  fp"{root}/b/same/q".write("same\n")?
  fp"{root}/a/sub/f".write("x\n")?
  fp"{root}/b/sub/f".write("y\n")?
  fp"{root}/a/only".write("left\n")?
  fp"{root}/b/ronly".write("right\n")?
  fp"{root}/a/bin".write(b"a\0b")?
  fp"{root}/b/bin".write(b"a\0c")?
  fp"{root}/a/fd".write("file\n")?
  fp"{root}/b/fd".mkdir()?
  Ok(root)
}

test test_diff_directories_without_recursion_lists_entries_and_common_subdirectories { |ctx|
  let root = build_trees(ctx, "diff-dirs")?
  let result = invoke(ctx, ["a", "b"], cwd: root)?
  assert result.status == 1, result.stderr
  assert result.stdout == "Binary files a/bin and b/bin differ\nFile a/fd is a regular file while file b/fd is a directory\nOnly in a: only\nOnly in b: ronly\nCommon subdirectories: a/same and b/same\nCommon subdirectories: a/sub and b/sub\n", result.stdout
}

test test_diff_recursive_prints_header_line_before_each_file_diff { |ctx|
  let root = build_trees(ctx, "diff-recursive")?
  let normal = invoke(ctx, ["-r", "a", "b"], cwd: root)?
  assert normal.status == 1, normal.stderr
  assert normal.stdout == "Binary files a/bin and b/bin differ\nFile a/fd is a regular file while file b/fd is a directory\nOnly in a: only\nOnly in b: ronly\ndiff -r a/sub/f b/sub/f\n1c1\n< x\n---\n> y\n", normal.stdout
  let unified = invoke(ctx, ["-ru", "a", "b"], cwd: root)?
  assert unified.status == 1
  assert unified.stdout.ends_with("diff -ru a/sub/f b/sub/f\n--- a/sub/f\n+++ b/sub/f\n@@ -1 +1 @@\n-x\n+y\n"), unified.stdout
}

test test_diff_recursive_identical_trees_exit_zero_and_report_with_s { |ctx|
  let root = test.temp_dir(ctx, name: "diff-identical")?
  fp"{root}/a/d".mkdir(parents: true)?
  fp"{root}/b/d".mkdir(parents: true)?
  fp"{root}/a/d/f".write("same\n")?
  fp"{root}/b/d/f".write("same\n")?
  let quiet = invoke(ctx, ["-r", "a", "b"], cwd: root)?
  assert quiet.status == 0, quiet.stderr
  assert quiet.stdout == ""
  let reported = invoke(ctx, ["-rs", "a", "b"], cwd: root)?
  assert reported.status == 0
  assert reported.stdout == "Files a/d/f and b/d/f are identical\n", reported.stdout
}

test test_diff_recursive_brief_names_differing_files { |ctx|
  let root = build_trees(ctx, "diff-brief")?
  let result = invoke(ctx, ["-rq", "a/sub", "b/sub"], cwd: root)?
  assert result.status == 1
  assert result.stdout == "Files a/sub/f and b/sub/f differ\n", result.stdout
}

test test_diff_new_file_compares_absent_entries_as_empty { |ctx|
  let root = build_trees(ctx, "diff-new-file")?
  let result = invoke(ctx, ["-rN", "a", "b"], cwd: root)?
  assert result.status == 1, result.stderr
  assert result.stdout.find("Only in") == null, result.stdout
  assert result.stdout.find("diff -rN a/only b/only\n1d0\n< left\n") != null, result.stdout
  assert result.stdout.find("diff -rN a/ronly b/ronly\n0a1\n> right\n") != null, result.stdout
}

test test_diff_exclude_patterns_and_exclude_file_skip_entries_by_name { |ctx|
  let root = build_trees(ctx, "diff-exclude")?
  let patterns = fp"{root}/patterns"
  patterns.write("o*\nr*\n")?
  let result = invoke(ctx, ["-r", "-x", "bin", "--exclude=fd", "-X", patterns.display(), "-x", "sub", "a", "b"], cwd: root)?
  assert result.status == 0, result.stdout
  assert result.stdout == ""
}

test test_diff_directory_against_file_compares_the_same_named_entry { |ctx|
  let root = build_trees(ctx, "diff-dir-file")?
  fp"{root}/y".write("y\n")?
  let result = invoke(ctx, ["a/sub", "y"], cwd: root)?
  assert result.status == 2, result.stdout
  assert result.stderr.find("diff: a/sub/y: No such file or directory") != null, result.stderr
  let same = invoke(ctx, ["b/sub", "b/sub/f"], cwd: root)?
  assert same.status == 0, same.stderr
  let stdin = invoke(ctx, ["-", "a"], b"", cwd: root)?
  assert stdin.status == 2
  assert stdin.stderr.find("cannot compare '-' to a directory") != null, stdin.stderr
}

test test_diff_recursive_symlink_loop_is_reported_not_followed { |ctx|
  let root = test.temp_dir(ctx, name: "diff-loop")?
  fp"{root}/a/x".mkdir(parents: true)?
  fp"{root}/b/x".mkdir(parents: true)?
  fp"{root}/a/x/up".symlink(to: p"..")?
  fp"{root}/b/x/up".symlink(to: p"..")?
  let result = invoke(ctx, ["-r", "a", "b"], cwd: root)?
  assert result.status == 2
  assert result.stderr.find("recursive directory loop") != null, result.stderr
}

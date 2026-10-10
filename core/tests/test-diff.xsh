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
  assert invoke(ctx, ["-r", present.display(), present.display()])?.status == 2
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

use support.uu as uu

# origin: busybox diff/diff of stdin
test test_bb_diff_diff_of_stdin_1ca9b4f3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\nasd\nzxc\n")?
  let r = uu.invoke(s, "diff", ["-u", "-", "input"], stdin: bytes.from_text("asd\n"), timeout: 5s)?
  uu.fails_with_code(r, 1)
  let normalized = [line.split("\t")[0] for line in r.stdout.utf8()?.split("\n")].join("\n")
  assert normalized == "--- -\n+++ input\n@@ -1 +1,3 @@\n+qwe\n asd\n+zxc\n"
}

# origin: busybox diff/diff of stdin, no newline in the file
test test_bb_diff_diff_of_stdin_no_newline_in_the_file_7c561a26 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "qwe\nasd\nzxc")?
  let r = uu.invoke(s, "diff", ["-u", "-", "input"], stdin: bytes.from_text("asd\n"), timeout: 5s)?
  uu.fails_with_code(r, 1)
  let normalized = [line.split("\t")[0] for line in r.stdout.utf8()?.split("\n")].join("\n")
  assert normalized == "--- -\n+++ input\n@@ -1 +1,3 @@\n+qwe\n asd\n+zxc\n\\ No newline at end of file\n"
}

# origin: busybox diff/diff of empty file against stdin
test test_bb_diff_diff_of_empty_file_against_stdin_f60287fd { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "")?
  let r = uu.invoke(s, "diff", ["-u", "-", "input"], stdin: bytes.from_text("a\n"), timeout: 5s)?
  uu.fails_with_code(r, 1)
  let normalized = [line.split("\t")[0] for line in r.stdout.utf8()?.split("\n")].join("\n")
  assert normalized == "--- -\n+++ input\n@@ -1 +0,0 @@\n-a\n"
}

# origin: busybox diff/diff of empty file against nonempty one
test test_bb_diff_diff_of_empty_file_against_nonempty_one_b4d1bf64 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a\n")?
  let r = uu.invoke(s, "diff", ["-u", "-", "input"], stdin: bytes.from_text(""), timeout: 5s)?
  uu.fails_with_code(r, 1)
  let normalized = [line.split("\t")[0] for line in r.stdout.utf8()?.split("\n")].join("\n")
  assert normalized == "--- -\n+++ input\n@@ -0,0 +1 @@\n+a\n"
}

# origin: busybox diff/diff -b treats EOF as whitespace
test test_bb_diff_diff_b_treats_EOF_as_whitespace_73c935a0 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abc")?
  let r = uu.invoke(s, "diff", ["-ub", "-", "input"], stdin: bytes.from_text("abc "), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox diff/diff -b treats all spaces as equal
test test_bb_diff_diff_b_treats_all_spaces_as_equal_3b8d241d { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a \t c\n")?
  let r = uu.invoke(s, "diff", ["-ub", "-", "input"], stdin: bytes.from_text("a\t \tc\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox diff/diff -B ignores changes whose lines are all blank
test test_bb_diff_diff_B_ignores_changes_whose_lines_are_all_blank_b6616169 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a\n")?
  let r = uu.invoke(s, "diff", ["-uB", "-", "input"], stdin: bytes.from_text("\na\n\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox diff/diff -B does not ignore changes whose lines are not all blank
test test_bb_diff_diff_B_does_not_ignore_changes_whose_lines_are_not_all_blank_5a5d5252 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a\n")?
  let r = uu.invoke(s, "diff", ["-uB", "-", "input"], stdin: bytes.from_text("\nb\n\n"), timeout: 5s)?
  uu.fails_with_code(r, 1)
  let normalized = [line.split("\t")[0] for line in r.stdout.utf8()?.split("\n")].join("\n")
  assert normalized == "--- -\n+++ input\n@@ -1,3 +1 @@\n-\n-b\n-\n+a\n"
}

# origin: busybox diff/diff -B ignores blank single line change
test test_bb_diff_diff_B_ignores_blank_single_line_change_7434ae1f { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "\n1\n")?
  let r = uu.invoke(s, "diff", ["-qB", "-", "input"], stdin: bytes.from_text("1\n"), timeout: 5s)?
  uu.fails_with_code(r, 0)
  uu.stdout_is(r, "")
}

# origin: busybox diff/diff -B does not ignore non-blank single line change
test test_bb_diff_diff_B_does_not_ignore_non_blank_single_line_change_e2409ce3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "0\n")?
  let r = uu.invoke(s, "diff", ["-qB", "-", "input"], stdin: bytes.from_text("1\n"), timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "Files - and input differ\n")
}

# origin: busybox diff/diff always takes context from old file
test test_bb_diff_diff_always_takes_context_from_old_file_93415550 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abc\na  c\ndef\n")?
  let r = uu.invoke(s, "diff", ["-ub", "-", "input"], stdin: bytes.from_text("a c\n"), timeout: 5s)?
  uu.fails_with_code(r, 1)
  let normalized = [line.split("\t")[0] for line in r.stdout.utf8()?.split("\n")].join("\n")
  assert normalized == "--- -\n+++ input\n@@ -1 +1,3 @@\n+abc\n a c\n+def\n"
}

# origin: busybox diff/diff of stdin, twice
test test_bb_diff_diff_of_stdin_twice_3a25d51e { |ctx|
  let s = uu.scene(ctx)?
  let words = uu.argv(s, "diff", [p"-", p"-"])?
  let shell = [p"/bin/sh", p"-c", p"\"$@\"; printf '%s\\n' \"$?\"; wc -c", p"diff-consumption"].extend(words)
  let output = uu.at(s, "stdout")
  let errors = uu.at(s, "stderr")
  let status = process.run(process.command_argv(p"/bin/sh", shell, s.root, {}, b"stdin", output, errors, timeout: 5s))?
  assert status.exit_code()? == 0
  assert output.read_bytes()? == b"0\n5\n"
}

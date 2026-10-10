# The diff applet is checked against results recorded from GNU diffutils 3.12.
# `data/diff/cases.txt` lists each case's arguments and the exact stdout,
# stderr and exit status GNU produced on the files under `data/diff/fixtures`,
# so the suite needs no GNU diff at test time. File times are pinned: a fixture
# whose name ends in 2 has one fixed modification time and every other file has
# another, so timestamps in unified and context headers are reproducible.

type Ran = {status: Int, stdout: Bytes, stderr: Bytes}

# One recorded case: the arguments, optional stdin fixture, the locale, whether header
# timestamps are scrubbed (stdin and device times vary), and the expected
# status and stream lines as stored.
type Recorded = {id: Str, args: List[Str], stdin: Str, locale: Str, scrub: Bool, status: Int, out: List[Str], err: List[Str]}

const TIME_FIRST = 1600000000123456789
const TIME_SECOND = 1700000000987654321

proc invoke(ctx: TestContext, args: List[Str], input = b"", cwd: Path? = null, label = "diff-capture", locale = "C") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: label)?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/diff.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, {LC_ALL: locale, TZ: "UTC", TERM: "xterm"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# The fixture tree copied to a scratch directory with the pinned times.
proc fixtures(ctx: TestContext, name: Str) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: name)?
  let _ = fs.copy_tree(fp"{ctx.core_dir}/tests/data/diff/fixtures", fp"{root}/fx")?
  for entry in fs.walk(fp"{root}/fx", hidden: true)? {
    if entry.kind == "file" {
      fs.set_times(entry.path, mtime_ns: if entry.name.ends_with("2") { TIME_SECOND } else { TIME_FIRST })?
    }
  }
  Ok(fp"{root}/fx")
}

pure hex_value(pair: Str) -> Int {
  let digits = "0123456789abcdef"
  (digits.find(pair.byte_slice(0, length: 1)) ?? 0) * 16 + (digits.find(pair.byte_slice(1, length: 1)) ?? 0)
}

# Decode the escapes of a stored line: `\\` and `\xHH`.
pure decode(text: Str) -> Bytes {
  if text.find("\\") == null { return bytes.from_text(text) }
  let raw = bytes.from_text(text)
  var values: List[Int] = []
  var at = 0
  while at < raw.len() {
    let value = raw.byte_at(at) ?? 0
    if value == 92 and raw.byte_at(at + 1) == 92 {
      values += [92]
      at += 2
    } else if value == 92 and raw.byte_at(at + 1) == 120 {
      values += [hex_value(raw[at + 2..at + 4].utf8() ?? "00")]
      at += 4
    } else {
      values += [value]
      at += 1
    }
  }
  bytes.from_ints(values) ?? b""
}

# The stream bytes of stored lines: a lower-case tag ends the line with a
# newline, an upper-case tag is a final line without one.
pure joined(lines: List[Str], small: Str) -> Bytes {
  var chunks: List[Bytes] = []
  for line in lines {
    chunks += [decode(line.byte_slice(2))]
    if line.starts_with(small) { chunks += [b"\n"] }
  }
  bytes.concat(chunks)
}

proc load(table: Path) [fs, error] -> Result[List[Recorded]] {
  var cases: List[Recorded] = []
  var current: Recorded? = null
  for line in table.lines()? {
    if line.starts_with("#") or line == "" { continue }
    if line.starts_with("@") {
      if let done = current { cases += [done] }
      let fields = line.byte_slice(1).split("\t")
      var arguments: List[Str] = []
      for field in fields[1..] { arguments += [decode(field).utf8() ?? ""] }
      current = {id: fields[0], args: arguments, stdin: "", locale: "C", scrub: false, status: 0, out: [], err: []}
      continue
    }
    let found = current ?? {id: "", args: [], stdin: "", locale: "C", scrub: false, status: 0, out: [], err: []}
    if line.starts_with("in ") {
      current = {...found, stdin: line.byte_slice(3)}
    } else if line.starts_with("lc ") {
      current = {...found, locale: line.byte_slice(3)}
    } else if line == "scrub" {
      current = {...found, scrub: true}
    } else if line.starts_with("status ") {
      current = {...found, status: line.byte_slice(7).parse_int() ?? 0}
    } else if line.starts_with("o:") or line.starts_with("O:") {
      current = {...found, out: found.out + [line]}
    } else if line.starts_with("e:") or line.starts_with("E:") {
      current = {...found, err: found.err + [line]}
    }
  }
  if let done = current { cases += [done] }
  Ok(cases)
}

# Header lines of unified and context diffs carry a time that is not pinned for
# standard input or devices; drop it on both sides.
pure scrubbed(data: Bytes) -> Bytes {
  var chunks: List[Bytes] = []
  for line in data.lines() {
    var cut = line.len()
    if line.starts_with(b"--- ") or line.starts_with(b"+++ ") or line.starts_with(b"*** ") {
      for index in range(line.len()) {
        if line.byte_at(index) == 9 and cut == line.len() { cut = index }
      }
    }
    chunks += [line[0..cut], b"\n"]
  }
  bytes.concat(chunks)
}

# Run one recorded case and return a failure description, or "" when the
# status and both streams match.
proc check(ctx: TestContext, root: Path, entry: Recorded) [fs, process, error] -> Str {
  let input = if entry.stdin == "" { b"" } else { fp"{root}/{entry.stdin}".read_bytes() ?? b"" }
  let result = invoke(ctx, entry.args, input, cwd: root, label: f"diff-{entry.id}", locale: entry.locale)
  if let Err(failure) = result { return f"{entry.id}: could not run: {failure.message}" }
  let ran = result ?? {status: -1, stdout: b"", stderr: b""}
  var expected_out = joined(entry.out, "o:")
  var actual_out = ran.stdout
  if entry.scrub {
    expected_out = scrubbed(expected_out)
    actual_out = scrubbed(actual_out)
  }
  let expected_err = joined(entry.err, "e:")
  if ran.status != entry.status or actual_out != expected_out or ran.stderr != expected_err {
    return f"{entry.id} (diff {entry.args.join(" ")}): status {ran.status} want {entry.status}\nstdout got {actual_out.dump()}\nstdout want {expected_out.dump()}\nstderr got {ran.stderr.dump()}\nstderr want {expected_err.dump()}"
  }
  ""
}

# Run every recorded case whose id starts with `prefix` and fail with the
# differences of up to three of them.
proc run_group(ctx: TestContext, prefix: Str) [fs, process, error] -> Result[Unit] {
  let root = fixtures(ctx, f"diff-{prefix}")?
  let all = load(fp"{ctx.core_dir}/tests/data/diff/cases.txt")?
  let chosen = [item for item in all if item.id.starts_with(prefix + "_")]
  assert !chosen.is_empty(), f"no recorded cases for {prefix}"
  let verdicts = chosen |> par-map(jobs: 8) { |item| check(ctx, root, item) } |> collect()
  let failures = [text for text in verdicts if text != ""]
  let shown = if failures.len() < 3 { failures.len() } else { 3 }
  assert failures.is_empty(), f"{failures.len()} of {chosen.len()} cases differ from GNU diff:\n" + failures[0..shown].join("\n\n")
  Ok()
}

test test_diff_recorded_output_styles { |ctx|
  run_group(ctx, "sty")?
}

test test_diff_recorded_ignore_options { |ctx|
  run_group(ctx, "ign")?
}

test test_diff_recorded_output_formatting { |ctx|
  run_group(ctx, "fmt")?
}

test test_diff_recorded_side_by_side_widths { |ctx|
  run_group(ctx, "wid")?
}

test test_diff_recorded_function_headings { |ctx|
  run_group(ctx, "fn")?
}

test test_diff_recorded_labels_binary_and_brief { |ctx|
  run_group(ctx, "lab")?
  run_group(ctx, "bin")?
  run_group(ctx, "brf")?
}

test test_diff_recorded_search_heuristics { |ctx|
  run_group(ctx, "alg")?
}

test test_diff_recorded_directories_and_operands { |ctx|
  run_group(ctx, "dir")?
  run_group(ctx, "opd")?
  run_group(ctx, "exc")?
  run_group(ctx, "stf")?
  run_group(ctx, "srt")?
}

test test_diff_recorded_standard_input { |ctx|
  run_group(ctx, "sin")?
}

test test_diff_recorded_errors_and_exit_statuses { |ctx|
  run_group(ctx, "err")?
}

test test_diff_recorded_merged_output { |ctx|
  run_group(ctx, "ifd")?
}

test test_diff_recorded_color { |ctx|
  run_group(ctx, "col")?
}

test test_diff_recorded_utf8_locale { |ctx|
  run_group(ctx, "mb")?
}

test test_diff_recorded_devices { |ctx|
  run_group(ctx, "dev")?
}

test test_diff_special_files_in_directories_are_not_read { |ctx|
  let root = test.temp_dir(ctx, name: "diff-special")?
  fp"{root}/f1".mkdir()?
  fp"{root}/f2".mkdir()?
  fs.mkfifo(fp"{root}/f1/p", 0o600)?
  fs.mkfifo(fp"{root}/f2/p", 0o600)?
  fp"{root}/f1/a".write("x\n")?
  fp"{root}/f2/a".write("y\n")?
  let result = invoke(ctx, ["-r", "f1", "f2"], cwd: root)?
  assert result.status == 1, result.stderr.utf8()?
  assert result.stdout.utf8()? == "diff -r f1/a f2/a\n1c1\n< x\n---\n> y\nFile f1/p is a fifo while file f2/p is a fifo\n", result.stdout.utf8()?
}

test test_diff_unreadable_operand_is_trouble { |ctx|
  if unix.id()?.euid == 0 {
    test.skip("root reads files regardless of their mode")
    return
  }
  let root = test.temp_dir(ctx, name: "diff-unreadable")?
  let secret = fp"{root}/secret"
  secret.write("x\n")?
  secret.chmod(0o000)?
  fp"{root}/ok".write("y\n")?
  let result = invoke(ctx, ["secret", "ok"], cwd: root)?
  assert result.status == 2
  assert result.stdout == b""
  assert result.stderr.utf8()? == "diff: secret: Permission denied\n", result.stderr.utf8()?
}

test test_diff_dangling_entries_report_the_first_name_only { |ctx|
  let root = test.temp_dir(ctx, name: "diff-dangling")?
  fp"{root}/a".mkdir()?
  fp"{root}/b".mkdir()?
  fp"{root}/a/l".symlink(to: p"nowhere")?
  fp"{root}/b/l".symlink(to: p"nowhere")?
  let result = invoke(ctx, ["a", "b"], cwd: root)?
  assert result.status == 2
  assert result.stderr.utf8()? == "diff: a/l: No such file or directory\n", result.stderr.utf8()?
}

# The tests below predate the recorded table and keep their names; where GNU
# prints a modification time after a file name the expectation drops it.
test test_diff_unified_normal_and_equal_status { |ctx|
  let left = test.temp_file(ctx, name: "left", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "right", contents: b"a\nc\n")?
  let unified = invoke(ctx, ["-u", left.display(), right.display()])?
  assert unified.status == 1
  assert unified.stdout.utf8()?.find("@@ -1,2 +1,2 @@") != null
  assert unified.stdout.utf8()?.find("-b\n+c\n") != null
  let normal = invoke(ctx, [left.display(), right.display()])?
  assert normal.status == 1
  assert normal.stdout.utf8()? == "2c2\n< b\n---\n> c\n"
  assert invoke(ctx, [left.display(), left.display()])?.status == 0
}

test test_diff_stdin_and_brief { |ctx|
  let file = test.temp_file(ctx, name: "right", contents: b"second\n")?
  let result = invoke(ctx, ["-q", "-", file.display()], b"first\n")?
  assert result.status == 1
  assert result.stdout.utf8()?.find("differ") != null
}

test test_diff_insert_delete_labels_and_binary { |ctx|
  let empty = test.temp_file(ctx, name: "empty")?
  let one = test.temp_file(ctx, name: "one", contents: b"one\n")?
  assert invoke(ctx, [empty.display(), one.display()])?.stdout.utf8()? == "0a1\n> one\n"
  assert invoke(ctx, [one.display(), empty.display()])?.stdout.utf8()? == "1d0\n< one\n"
  let labeled = invoke(ctx, ["-u", "-L", "before", "-L", "after", empty.display(), one.display()])?
  assert labeled.stdout.utf8()?.starts_with("--- before\n+++ after\n")
  let binary_file = test.temp_file(ctx, name: "binary", contents: b"one\0")?
  assert invoke(ctx, [binary_file.display(), one.display()])?.stdout.utf8()?.starts_with("Binary files ")
}

test test_diff_missing_file_and_option_conflicts_fail { |ctx|
  let present = test.temp_file(ctx, name: "present", contents: b"x")?
  let missing = test.temp_path(ctx, name: "missing")
  assert invoke(ctx, [present.display(), missing.display()])?.status == 2
  let conflict = invoke(ctx, ["-y", "-u", present.display(), present.display()])?
  assert conflict.status == 2
  assert conflict.stderr.utf8()? == "diff: conflicting output style options\ndiff: Try 'diff --help' for more information.\n", conflict.stderr.utf8()?
}

test test_diff_unified_short_context_takes_separate_value { |ctx|
  let left = test.temp_file(ctx, name: "context-left", contents: b"a\nb\n")?
  let right = test.temp_file(ctx, name: "context-right", contents: b"a\nc\n")?
  let result = invoke(ctx, ["-U", "0", left.display(), right.display()])?
  assert result.status == 1, result.stderr.utf8()?
  assert result.stdout.utf8()?.find("@@ -2 +2 @@") != null
}

test test_diff_ignore_space_change_compares_whitespace_runs { |ctx|
  let old = test.temp_file(ctx, name: "space-old", contents: b"a \t c\n")?
  let same = invoke(ctx, ["-ub", old.display(), "-"], b"a\t \tc\n")?
  assert same.status == 0, same.stderr.utf8()?
  assert same.stdout == b""

  let trailing = invoke(ctx, ["-ub", old.display(), "-"], b"a \t c   \n")?
  assert trailing.status == 0

  let indented = invoke(ctx, ["-b", old.display(), "-"], b" a \t c\n")?
  assert indented.status == 1, "leading whitespace is a change"
}

test test_diff_ignore_space_change_at_end_of_file { |ctx|
  let old = test.temp_file(ctx, name: "eof-old", contents: b"abc")?
  let result = invoke(ctx, ["-ub", old.display(), "-"], b"abc ")?
  assert result.status == 0, result.stderr.utf8()?
  assert result.stdout == b""
}

test test_diff_ignore_space_change_takes_context_from_old_file { |ctx|
  let input = test.temp_file(ctx, name: "context-new", contents: b"abc\na  c\ndef\n")?
  let result = invoke(ctx, ["-ub", "-L", "old", "-L", "new", "-", input.display()], b"a c\n")?
  assert result.status == 1, result.stderr.utf8()?
  assert result.stdout.utf8()? == "--- old\n+++ new\n@@ -1 +1,3 @@\n+abc\n a c\n+def\n", result.stdout.utf8()?
}

test test_diff_ignore_blank_lines_drops_only_blank_changes { |ctx|
  let input = test.temp_file(ctx, name: "blank-new", contents: b"a\n")?
  let blank_only = invoke(ctx, ["-uB", "-", input.display()], b"\na\n\n")?
  assert blank_only.status == 0, blank_only.stderr.utf8()?
  assert blank_only.stdout == b""

  let mixed = invoke(ctx, ["-uB", "-L", "old", "-L", "new", "-", input.display()], b"\nb\n\n")?
  assert mixed.status == 1
  assert mixed.stdout.utf8()? == "--- old\n+++ new\n@@ -1,3 +1 @@\n-\n-b\n-\n+a\n", mixed.stdout.utf8()?
}

test test_diff_ignore_blank_lines_brief_and_single_line { |ctx|
  let input = test.temp_file(ctx, name: "single-new", contents: b"1\n")?
  let blank = invoke(ctx, ["-qB", "-", input.display()], b"\n1\n")?
  assert blank.status == 0
  assert blank.stdout == b""

  let changed = test.temp_file(ctx, name: "single-changed", contents: b"0\n")?
  let text = invoke(ctx, ["-qB", "-", changed.display()], b"1\n")?
  assert text.status == 1
  assert text.stdout.utf8()? == "Files - and " + changed.display() + " differ\n", text.stdout.utf8()?
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
  assert result.status == 1, result.stderr.utf8()?
  assert result.stdout.utf8()? == "Binary files a/bin and b/bin differ\nFile a/fd is a regular file while file b/fd is a directory\nOnly in a: only\nOnly in b: ronly\nCommon subdirectories: a/same and b/same\nCommon subdirectories: a/sub and b/sub\n", result.stdout.utf8()?
}

test test_diff_recursive_prints_header_line_before_each_file_diff { |ctx|
  let root = build_trees(ctx, "diff-recursive")?
  let normal = invoke(ctx, ["-r", "a", "b"], cwd: root)?
  assert normal.status == 1, normal.stderr.utf8()?
  assert normal.stdout.utf8()? == "Binary files a/bin and b/bin differ\nFile a/fd is a regular file while file b/fd is a directory\nOnly in a: only\nOnly in b: ronly\ndiff -r a/sub/f b/sub/f\n1c1\n< x\n---\n> y\n", normal.stdout.utf8()?
  let unified = invoke(ctx, ["-ru", "-L", "one", "-L", "two", "a", "b"], cwd: root)?
  assert unified.status == 1
  assert unified.stdout.utf8()?.ends_with("diff -ru -L one -L two one two\n--- one\n+++ two\n@@ -1 +1 @@\n-x\n+y\n"), unified.stdout.utf8()?
}

test test_diff_recursive_identical_trees_exit_zero_and_report_with_s { |ctx|
  let root = test.temp_dir(ctx, name: "diff-identical")?
  fp"{root}/a/d".mkdir(parents: true)?
  fp"{root}/b/d".mkdir(parents: true)?
  fp"{root}/a/d/f".write("same\n")?
  fp"{root}/b/d/f".write("same\n")?
  let quiet = invoke(ctx, ["-r", "a", "b"], cwd: root)?
  assert quiet.status == 0, quiet.stderr.utf8()?
  assert quiet.stdout == b""
  let reported = invoke(ctx, ["-rs", "a", "b"], cwd: root)?
  assert reported.status == 0
  assert reported.stdout.utf8()? == "Files a/d/f and b/d/f are identical\n", reported.stdout.utf8()?
}

test test_diff_recursive_brief_names_differing_files { |ctx|
  let root = build_trees(ctx, "diff-brief")?
  let result = invoke(ctx, ["-rq", "a/sub", "b/sub"], cwd: root)?
  assert result.status == 1
  assert result.stdout.utf8()? == "Files a/sub/f and b/sub/f differ\n", result.stdout.utf8()?
}

test test_diff_new_file_compares_absent_entries_as_empty { |ctx|
  let root = build_trees(ctx, "diff-new-file")?
  let result = invoke(ctx, ["-rN", "a", "b"], cwd: root)?
  assert result.status == 1, result.stderr.utf8()?
  let text = result.stdout.utf8()?
  assert text.find("Only in") == null, text
  assert text.find("diff -rN a/only b/only\n1d0\n< left\n") != null, text
  assert text.find("diff -rN a/ronly b/ronly\n0a1\n> right\n") != null, text
}

test test_diff_exclude_patterns_and_exclude_file_skip_entries_by_name { |ctx|
  let root = build_trees(ctx, "diff-exclude")?
  let patterns = fp"{root}/patterns"
  patterns.write("o*\nr*\n")?
  let result = invoke(ctx, ["-r", "-x", "bin", "--exclude=fd", "-X", patterns.display(), "-x", "sub", "a", "b"], cwd: root)?
  assert result.status == 0, result.stdout.utf8()?
  assert result.stdout == b""
}

test test_diff_directory_against_file_compares_the_same_named_entry { |ctx|
  let root = build_trees(ctx, "diff-dir-file")?
  fp"{root}/y".write("y\n")?
  let result = invoke(ctx, ["a/sub", "y"], cwd: root)?
  assert result.status == 2, result.stdout.utf8()?
  assert result.stderr.utf8()?.find("diff: a/sub/y: No such file or directory") != null, result.stderr.utf8()?
  let same = invoke(ctx, ["b/sub", "b/sub/f"], cwd: root)?
  assert same.status == 0, same.stderr.utf8()?
  let stdin = invoke(ctx, ["-", "a"], b"", cwd: root)?
  assert stdin.status == 2
  assert stdin.stderr.utf8()?.find("cannot compare '-' to a directory") != null, stdin.stderr.utf8()?
}

test test_diff_recursive_symlink_loop_is_reported_not_followed { |ctx|
  let root = test.temp_dir(ctx, name: "diff-loop")?
  fp"{root}/a/x".mkdir(parents: true)?
  fp"{root}/b/x".mkdir(parents: true)?
  fp"{root}/a/x/up".symlink(to: p"..")?
  fp"{root}/b/x/up".symlink(to: p"..")?
  let result = invoke(ctx, ["-r", "a", "b"], cwd: root)?
  assert result.status == 2
  assert result.stderr.utf8()?.find("recursive directory loop") != null, result.stderr.utf8()?
}

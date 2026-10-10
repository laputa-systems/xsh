test test_grep_basic_prefix_count_and_exit_status { |ctx|
  let root = test.temp_dir(ctx, name: "grep-basic")?
  let file = fp"{root}/input"
  file.write("red\nblue\nred blue\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -n red $file
  assert out == "1:red\n3:red blue\n"
  let observed1 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -vc red $file
  assert observed1 == "1\n"
  let absent = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- missing $file
  assert absent.status.exited_with(1)
}

test test_grep_bre_backreference_and_ere_alternation { |ctx|
  let root = test.temp_dir(ctx, name: "grep-regex")?
  let file = fp"{root}/input"
  file.write("abab\nabac\nred\nblue\n")
  let observed2 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- "\\(ab\\)\\1" $file
  assert observed2 == "abab\n"
  let observed3 = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -E "^(red|blue)$" $file
  assert observed3 == "red\nblue\n"
}

test test_grep_fixed_binary_and_nul_records { |ctx|
  let root = test.temp_dir(ctx, name: "grep-bytes")?
  let file = fp"{root}/input"
  file.write(b"a.b\xff\naxb\n")
  let out = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fa a.b $file
  assert out == b"a.b\xff\n"
  file.write(b"one\0two\0")
  let observed4 = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fz two $file
  assert observed4 == b"two\0"
}

test test_grep_recursive_word_and_context { |ctx|
  let root = test.temp_dir(ctx, name: "grep-recursive")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  fp"{dir}/file".write("before\nred\nafter\nredder\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -hw -A1 red fp"{dir}/file"
  assert out == "red\nafter\n"
  let files = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -rl red $dir
  assert files == f"{dir}/file\n"
}

test test_grep_pattern_files_only_matching_and_invalid_expression { |ctx|
  let root = test.temp_dir(ctx, name: "grep-patterns")?
  let file = fp"{root}/input"
  let patterns = fp"{root}/patterns"
  file.write("red-blue red\nother\n")
  patterns.write("red\nblue\n")
  let out = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fo -f $patterns $file
  assert out == "red\nblue\nred\n"
  let invalid = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- "[" $file
  assert invalid.status.exited_with(2)
  assert invalid.stdout == ""
}

test test_grep_count_zero_binary_and_color { |ctx|
  let root = test.temp_dir(ctx, name: "grep-selection")?
  let file = fp"{root}/input"
  file.write("red\nblue\n")
  # GNU grep exits at once, printing nothing, when -m is zero.
  let zero = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -cm0 red $file
  assert zero.status.exited_with(1)
  assert zero.stdout == ""
  let colored = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- --color=always red $file
  assert colored == b"\x1b[01;31m\x1b[Kred\x1b[m\x1b[K\n"
  file.write(b"red\0blue\n")
  # GNU grep 3.5 and later reports a binary match on stderr, not stdout.
  let binary = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -F red $file
  assert binary.status.exited_with(0)
  assert binary.stdout == ""
  assert binary.stderr == f"grep: {file}: binary file matches\n"
}

test test_grep_byte_offsets_nul_names_and_recursive_filters { |ctx|
  let root = test.temp_dir(ctx, name: "grep-offset")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let file = fp"{dir}/keep.txt"
  file.write("blue\nred red\n")
  fp"{dir}/skip.log".write("red\n")
  let offsets = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Fbo red $file
  assert offsets == "5:red\n9:red\n"
  let names = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -rlZ --include=*.txt red $dir
  assert names == bytes.from_text(f"{file}\0")
}

test test_grep_errors_and_quiet_match_precedence { |ctx|
  let root = test.temp_dir(ctx, name: "grep-errors")?
  let file = fp"{root}/file"
  file.write("selected\n")
  let directory = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- selected $root
  assert directory.status.exited_with(2)
  let absent = fp"{root}/absent"
  let quiet = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -q selected $absent $file
  assert quiet.status.exited_with(0)
  assert quiet.stdout == ""
  let suppressed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -s selected $absent
  assert suppressed.status.exited_with(2)
  assert suppressed.stderr == ""
  let invalid = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- --count=ignored selected $file
  assert invalid.status.exited_with(2)
}

test test_grep_recursive_symlink_policy_and_loop_error { |ctx|
  let root = test.temp_dir(ctx, name: "grep-links")?
  let dir = fp"{root}/dir"
  dir.mkdir()
  let file = fp"{dir}/file"
  file.write("needle\n")
  fp"{dir}/alias".symlink(to: p"file")
  let shallow = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -rl needle $dir
  assert shallow == f"{file}\n"
  fp"{dir}/loop".symlink(to: p".")
  # A directory loop is only a warning; the files before it are still listed.
  let followed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -Rl needle $dir
  assert followed.status.exited_with(0)
  assert f"{file}\n" in followed.stdout
  assert f"{dir}/alias\n" in followed.stdout
  assert followed.stdout.split("\n").len() == 3
  assert followed.stderr == f"grep: {dir}/loop: warning: recursive directory loop\n"
}

test test_grep_files_without_match_exit_status { |ctx|
  let root = test.temp_dir(ctx, name: "grep-files-without-match")?
  let file = fp"{root}/input"
  file.write("asd\n")
  let listed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -L qwe $file
  assert listed.status.exited_with(0)
  assert listed.stdout == f"{file}\n"
  file.write("qwe\n")
  let matched = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -L qwe $file
  assert matched.status.exited_with(1)
  assert matched.stdout == ""
}

# Differential table: every case in data/grep/cases.jsonl ran under GNU grep
# 3.12 once; data/grep/expected.txt holds its status, stdout and stderr
# (`regen.xsh` rebuilds that file). Each case runs the applet from inside the
# fixture tree, so names print as they did under GNU grep.
type Case = {id: Str, cmd: Str, args: List[Str], stdin: Str, lc: Str, colors: Str, color: Str, posixly: Bool, sort: Bool, lstatus: Bool}
type Outcome = {id: Str, status: Int, out: Str, err: Str}

# Bytes as one printable line: backslash escapes for the characters that
# would break the line format, `\xHH` for the rest.
pure escape(data: Bytes) -> Str {
  let digits = "0123456789abcdef"
  var pieces: List[Str] = []
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte == 92 { pieces += ["\\\\"] } else if byte == 10 { pieces += ["\\n"] } else if byte == 9 { pieces += ["\\t"] } else if byte == 13 { pieces += ["\\r"] } else if byte == 0 { pieces += ["\\0"] } else if byte >= 32 and byte < 127 { pieces += [data.slice(index, length: 1).utf8() ?? "?"] } else {
      pieces += [f"\\x{digits.byte_slice(byte / 16, 1)}{digits.byte_slice(byte % 16, 1)}"]
    }
  }
  pieces.join("")
}

# Directory listing order is the file system's, so recursive cases compare
# their records as a set (NUL-separated names count as records too).
pure ordered(text: Str, enabled: Bool) -> Str {
  if ! enabled { return text }
  let records = text.replace("\\0", with: "\\n").split("\\n")
  let sorted = records |> sort() |> collect()
  sorted.join("\\n")
}

proc load_cases(ctx: TestContext, family: Str) [fs, error] -> Result[List[Case]] {
  let source = fp"{ctx.core_dir}/tests/data/grep/cases.jsonl"
  var cases: List[Case] = []
  for line in source.read_lines()? {
    let doc = json.decode(line)?
    if json.get(doc, ["group"])?.require(Str)? != family { continue }
    cases += [{
      id: json.get(doc, ["id"])?.require(Str)?,
      cmd: json.get(doc, ["cmd"])?.require(Str)?,
      args: json.get(doc, ["args"])?.require(List[Str])?,
      stdin: json.get(doc, ["stdin"])?.require(Str)?,
      lc: json.get(doc, ["lc"])?.require(Str)?,
      colors: json.get(doc, ["colors"])?.require(Str)?,
      color: json.get(doc, ["color"])?.require(Str)?,
      posixly: json.get(doc, ["posixly"])?.require(Bool)?,
      sort: json.get(doc, ["sort"])?.require(Bool)?,
      lstatus: json.get(doc, ["lstatus"])?.require(Bool)?,
    }]
  }
  Ok(cases)
}

proc expected_outcomes(ctx: TestContext) [fs, error] -> Result[Map[Outcome]] {
  let source = fp"{ctx.core_dir}/tests/data/grep/expected.txt"
  var found: Map[Outcome] = {}
  var id = ""
  var status = 0
  var out = ""
  for line in source.read_lines()? {
    if line.starts_with("@ ") { id = line.byte_slice(2) } else if line.starts_with("rc ") { status = line.byte_slice(3).parse_int() ?? -1 } else if line.starts_with("out ") { out = line.byte_slice(4) } else if line.starts_with("err ") {
      found = {...found, [id]: {id: id, status: status, out: out, err: line.byte_slice(4)}}
    }
  }
  Ok(found)
}

proc run_case(ctx: TestContext, scratch: Path, c: Case) [fs, process, error] -> Outcome {
  let fixtures = fp"{ctx.core_dir}/tests/data/grep/fx"
  let script = fp"{ctx.core_dir}/{c.cmd}.xsh"
  let out = fp"{scratch}/{c.id}.out"
  let err = fp"{scratch}/{c.id}.err"
  let input = if c.stdin == "" { p"/dev/null" } else { fp"{fixtures}/{c.stdin}" }
  let argv = [ctx.xsh_bin.display(), script.display(), "--"].extend(c.args)
  let plan = if c.posixly {
    process.command_argv(ctx.xsh_bin, argv, fixtures, {LC_ALL: c.lc, GREP_COLORS: c.colors, GREP_COLOR: c.color, POSIXLY_CORRECT: "1"}, input, out, err)
  } else {
    process.command_argv(ctx.xsh_bin, argv, fixtures, {LC_ALL: c.lc, GREP_COLORS: c.colors, GREP_COLOR: c.color}, input, out, err)
  }
  let status = process.run(plan)?
  Outcome(id: c.id, status: status.exit_code()?, out: escape(out.read_bytes()?), err: escape(err.read_bytes()?))
}

proc check_group(ctx: TestContext, family: Str) [fs, process, error] {
  let cases = load_cases(ctx, family)?
  assert ! cases.is_empty(), f"no cases in group {family}"
  let expected = expected_outcomes(ctx)?
  let scratch = test.temp_dir(ctx, name: f"grep-{family}")?
  let actual: List[Outcome] = cases |> par-map(jobs: 12) { |c| run_case(ctx, scratch, c) } |> collect()
  var failures: List[Str] = []
  for index in range(cases.len()) {
    let c = cases[index]
    let got = actual[index]
    let want = expected.get(c.id) ?? {id: c.id, status: -1, out: "<missing>", err: "<missing>"}
    # BusyBox requires -L to exit 0 when it listed a file, the inverse of
    # GNU grep 3.5 and later, which reports whether a line was selected.
    var status = want.status
    if c.lstatus and status != 2 { status = if want.out == "" { 1 } else { 0 } }
    if got.status != status or ordered(got.out, c.sort) != ordered(want.out, c.sort) or ordered(got.err, c.sort) != ordered(want.err, c.sort) {
      failures += [f"{c.id}: {c.args.join(" ")}\n  want status {status} out {want.out} err {want.err}\n  got  status {got.status} out {got.out} err {got.err}"]
    }
  }
  assert failures.is_empty(), f"{failures.len()} of {cases.len()} differ:\n{failures.join("\n")}"
}

test test_grep_differential_basic_selection_and_modes { |ctx| check_group(ctx, "basic")? }
test test_grep_differential_basic_regular_expressions { |ctx| check_group(ctx, "bre")? }
test test_grep_differential_extended_regular_expressions { |ctx| check_group(ctx, "ere")? }
test test_grep_differential_fixed_words_lines_and_case { |ctx| check_group(ctx, "rxo")? }
test test_grep_differential_input_shapes_and_binary_files { |ctx| check_group(ctx, "in")? }
test test_grep_differential_binary_detection_details { |ctx| check_group(ctx, "bin")? }
test test_grep_differential_locales_and_multibyte_text { |ctx| check_group(ctx, "loc")? }
test test_grep_differential_context_and_separators { |ctx| check_group(ctx, "ctx")? }
test test_grep_differential_operands_stdin_and_exit_status { |ctx| check_group(ctx, "files")? }
test test_grep_differential_recursion_and_file_filters { |ctx| check_group(ctx, "rec")? }
test test_grep_differential_color_output { |ctx| check_group(ctx, "color")? }
test test_grep_differential_grep_colors_capabilities { |ctx| check_group(ctx, "colors")? }
test test_grep_differential_option_parsing_and_diagnostics { |ctx| check_group(ctx, "opt")? }
test test_grep_differential_exit_statuses { |ctx| check_group(ctx, "exit")? }
test test_grep_differential_null_data_records { |ctx| check_group(ctx, "z")? }
test test_grep_differential_egrep_wrapper { |ctx| check_group(ctx, "egrep")? }
test test_grep_differential_fgrep_wrapper { |ctx| check_group(ctx, "fgrep")? }
test test_grep_differential_generated_expressions { |ctx| check_group(ctx, "fz")? }

test test_grep_perl_regexp_is_refused_explicitly { |ctx|
  let root = test.temp_dir(ctx, name: "grep-perl")?
  let file = fp"{root}/input"
  file.write("abc\n")
  let refused = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -P "a(?=b)" $file
  assert refused.status.exited_with(2)
  assert refused.stdout == ""
  assert refused.stderr == "grep: Perl matching not supported in a --disable-perl-regexp build\n"
}

test test_grep_unreadable_file_is_reported_and_the_search_goes_on { |ctx|
  let root = test.temp_dir(ctx, name: "grep-unreadable")?
  let closed = fp"{root}/closed"
  let open = fp"{root}/open"
  closed.write("needle\n")
  open.write("needle\n")
  closed.chmod(0o000)?
  if closed.read_bytes() is Ok(_) { return }
  let ran = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- needle $closed $open
  assert ran.status.exited_with(2)
  assert ran.stdout == f"{open}:needle\n"
  assert ran.stderr == f"grep: {closed}: Permission denied\n"
  let quiet = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -s needle $closed $open
  assert quiet.status.exited_with(2)
  assert quiet.stderr == ""
}

# GNU grep inspects input one 96 KiB buffer at a time, so a NUL that first
# appears in a later buffer stops the output there: lines of the earlier
# buffers are printed, and the partial line carried into the buffer with the
# NUL is part of the binary report. Hundred-byte lines put the cut after
# line 982.
test test_grep_binary_detection_works_per_buffer { |ctx|
  let root = test.temp_dir(ctx, name: "grep-buffers")?
  let file = fp"{root}/input"
  let padding = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
  var data: List[Bytes] = []
  for index in range(1100) {
    data += [bytes.from_text(f"line {index:05} match {padding.byte_slice(0, 82)}\n")]
  }
  file.write(bytes.concat(data + [b"tail\0 match\n"]))
  let ran = run.capture --bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- match $file
  assert ran.status.exited_with(0)
  assert ran.stdout.len() == 98300
  assert ran.stderr == bytes.from_text(f"grep: {file}: binary file matches\n")
  let counted = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -c match $file
  assert counted == "1101\n"
}

test test_grep_files_without_match_status_follows_the_listing { |ctx|
  # The BusyBox suite requires -L to succeed when it listed a file, the
  # inverse of GNU grep 3.5 and later; the listing itself is GNU's.
  let root = test.temp_dir(ctx, name: "grep-L-status")?
  let hit = fp"{root}/hit"
  let miss = fp"{root}/miss"
  hit.write("needle\n")
  miss.write("other\n")
  let listed = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -L needle $hit $miss
  assert listed.stdout == f"{miss}\n"
  assert listed.status.exited_with(0)
  let none = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/grep.xsh" -- -L needle $hit
  assert none.stdout == ""
  assert none.status.exited_with(1)
}

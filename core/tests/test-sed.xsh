test sed_substitution_addresses_and_backreferences { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"one one\ntwo two\nthree three\nfour four\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let substitution = run.text ${ctx.xsh_bin} $app -- -E "2,3s/([a-z]+) ([a-z]+)/\\2-\\1/" $file
  assert substitution == "one one\ntwo-two\nthree-three\nfour four\n"
  let address_script = "/two/,/three/p;$p"
  let addresses = run.text ${ctx.xsh_bin} $app -- -n $address_script $file
  assert addresses == "two two\nthree three\nfour four\n"
  let occurrence = run.text ${ctx.xsh_bin} $app -- "s/o/X/2g" $file
  assert occurrence == "one Xne\ntwo twX\nthree three\nfour fXur\n"
}

test sed_spaces_branches_and_groups { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"one\ntwo\nthree\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let hold = run.text ${ctx.xsh_bin} $app -- -n "1h;2{G;p};3{x;p}" $file
  assert hold == "two\none\none\n"
  let pair = run.text ${ctx.xsh_bin} $app -- "N;s/\\n/:/;P;D" $file
  assert pair == "one:two\nthree\n"
  let repeated = test.temp_file(ctx, name: "repeated", contents: b"booooook\n")?
  let branch = run.text ${ctx.xsh_bin} $app -- ":again;s/oo/o/;t again" $repeated
  assert branch == "bok\n"
  let invert = run.text ${ctx.xsh_bin} $app -- "2!d" $file
  assert invert == "two\n"
}

test sed_text_scripts_and_inplace { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"one\ntwo\nthree")?
  let script = test.temp_file(ctx, name: "script", contents: b"1i\\\nbegin\n2c\\\nchanged\n$a\\\nend\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let text = run.text ${ctx.xsh_bin} $app -- -f $script $file
  assert text == "begin\none\nchanged\nthree\nend\n"
  run ${ctx.xsh_bin} $app -- -i.bak -e "s/one/ONE/" $file
  assert file.read_bytes()? == b"ONE\ntwo\nthree"
  assert fp"{file}.bak".read_bytes()? == b"one\ntwo\nthree"
}

test sed_basic_regex_bytes_and_errors { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"ab ab\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let basic = run.text ${ctx.xsh_bin} $app -- "s/\\(ab\\) \\1/[\\1]/" $file
  assert basic == "[ab]\n"
  let numeric_script = "s/\\x61/\\d65\\o102\\x43/"
  let numeric = run.text ${ctx.xsh_bin} $app -- $numeric_script $file
  assert numeric == "ABCb ab\n"
  let raw = test.temp_file(ctx, name: "raw", contents: b"a\0b\n")?
  let raw_output = run.bytes ${ctx.xsh_bin} $app -- -n "1p" $raw
  assert raw_output == b"a\0b\n"
  let unsupported = run.capture --text ${ctx.xsh_bin} $app -- "s/a/b/q" $file
  assert unsupported.status.exited_with(1)
  assert "char 7: unknown option to 's'" in unsupported.stderr
  let malformed = run.capture --text ${ctx.xsh_bin} $app -- "{p" $file
  assert malformed.status.exited_with(1)
  assert "char 0: unmatched '{'" in malformed.stderr
}


test sed_unterminated_hold_and_unclosed_change_range { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let swapped = run.bytes ${ctx.xsh_bin} $app -- -n "x;p" $file
  assert swapped == b"\n"
  let appended = run.bytes ${ctx.xsh_bin} $app -- "G" $file
  assert appended == b"a\n\n"
  let changed = run.bytes ${ctx.xsh_bin} $app -- "1,3c changed" $file
  assert changed == b""
  let stopped = run.bytes ${ctx.xsh_bin} $app -- "q" $file
  assert stopped == b"a\n"
}


test sed_empty_regex_reuses_last_executed_expression { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "/a/s/b/X/;s//Y/" $file
  assert output == "a\nb\n"
}


test sed_missing_input_reports_file_error_status { |ctx|
  let missing = test.temp_file(ctx, name: "missing-sed-input", contents: b"")?
  missing.remove()
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "p" $missing
  assert result.status.exited_with(2)
  assert "can't read" in result.stderr
}


test sed_multiple_unterminated_file_boundaries { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"a")?
  let second = test.temp_file(ctx, name: "second", contents: b"b")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let script = "s/$/X/"
  let edited = run.bytes ${ctx.xsh_bin} $app -- $script $first $second
  assert edited == b"aX\nbX"
  let selected = run.bytes ${ctx.xsh_bin} $app -- -n "1p" $first $second
  assert selected == b"a"
}

test sed_clustered_and_attached_option_values { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"ab\n")?
  let script = test.temp_file(ctx, name: "script", contents: b"s/b/B/\np\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let attached = "-f" + script.display()
  let short = run.text ${ctx.xsh_bin} $app -- -nE "-es/(a)/[\\1]/" $attached $file
  assert short == "[a]B\n"
  let long = run.text ${ctx.xsh_bin} $app -- "--expression=s/a/A/" --quiet --file $script $file
  assert long == "AB\n"
}

test sed_empty_unterminated_prints_emit_separators { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a")?
  let output = run.bytes ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- -n "z;p;p" $file
  assert output == b"\n"
}

test sed_numeric_stride_addresses { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"1\n2\n3\n4\n5\n6\n7\n8\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let zero_origin = run.text ${ctx.xsh_bin} $app -- -n "0~3p" $file
  assert zero_origin == "3\n6\n"
  let offset = run.text ${ctx.xsh_bin} $app -- -n "2~3p" $file
  assert offset == "2\n5\n8\n"
  let zero_step = run.text ${ctx.xsh_bin} $app -- -n "5~0p" $file
  assert zero_step == "5\n"
}

test sed_append_text_at_address { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2a added" $file
  assert output == "a\nb\nadded\nc\nd\n"
}

test sed_append_text_at_negated_address { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2!a added" $file
  assert output == "a\nadded\nb\nc\nadded\nd\nadded\n"
}

test sed_append_text_with_quiet_mode { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- -n "2a added" $file
  assert output == "added\n"
}

test sed_insert_text_before_address { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2i inserted" $file
  assert output == "a\ninserted\nb\nc\nd\n"
}

test sed_insert_text_at_negated_address { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2!i inserted" $file
  assert output == "inserted\na\nb\ninserted\nc\ninserted\nd\n"
}

test sed_change_text_at_one_address { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2c changed" $file
  assert output == "a\nchanged\nc\nd\n"
}

test sed_change_text_replaces_an_address_range_once { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2,3c changed" $file
  assert output == "a\nchanged\nd\n"
}

test sed_change_text_with_quiet_mode { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- -n "2,3c changed" $file
  assert output == "changed\n"
}

test sed_change_text_on_the_complement_of_a_range { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2,3!c changed" $file
  assert output == "changed\nb\nc\nchanged\n"
}

test sed_change_text_on_the_complement_with_quiet_mode { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\nd\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- -n "2,3!c changed" $file
  assert output == "changed\nchanged\n"
}

test sed_unclosed_change_range_emits_no_replacement { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- "2,5c changed" $file
  assert output == "a\n"
}

test sed_append_command_accepts_a_range { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let appended = run.text ${ctx.xsh_bin} $app -- "1,2a added" $file
  assert appended == "a\nadded\nb\nadded\nc\n"
  let negated = run.text ${ctx.xsh_bin} $app -- "1,2!a added" $file
  assert negated == "a\nb\nc\nadded\n"
}

test sed_insert_command_accepts_a_range { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let inserted = run.text ${ctx.xsh_bin} $app -- "1,2i inserted" $file
  assert inserted == "inserted\na\ninserted\nb\nc\n"
  let spaced = run.text ${ctx.xsh_bin} $app -- "1,2 i inserted" $file
  assert spaced == inserted
}

test sed_text_commands_join_escaped_lines { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\n")?
  let script = test.temp_file(ctx, name: "script", contents: b"1a\\\nfirst\\\nsecond\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/sed.xsh" -- -f $script $file
  assert output == "a\nfirst\nsecond\nb\n"
}

test sed_bracket_expression_holds_the_delimiter { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"one@two\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let bracketed = run.text ${ctx.xsh_bin} $app -- "s@[@]@@" $file
  assert bracketed == "onetwo\n"
  let spaced = test.temp_file(ctx, name: "spaced", contents: b" a.b\n")?
  let members = run.text ${ctx.xsh_bin} $app -- "s [^ .]* x g" $spaced
  assert members == "x x.x\n"
}

test sed_text_commands_expand_control_escapes { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"line1\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let appended = run.bytes ${ctx.xsh_bin} $app -- "1a a\\tb\\rc\\nd" $file
  assert appended == b"line1\na\tb\rc\nd\n"
  let inserted = run.bytes ${ctx.xsh_bin} $app -- "1i x\\ty" $file
  assert inserted == b"x\ty\nline1\n"
}

test sed_numeric_range_opens_once_past_a_consumed_start_line { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"first\nsecond\nthird\nfourth\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let skipped = run.text ${ctx.xsh_bin} $app -- -n "1d;1,3p" $file
  assert skipped == "second\nthird\n"
  let reversed = run.text ${ctx.xsh_bin} $app -- -n "2d;2,1p" $file
  assert reversed == "third\n"
  let numbers = test.temp_file(ctx, name: "numbers", contents: b"1\n2\n3\n4\n5\n")?
  let closed = run.text ${ctx.xsh_bin} $app -- -n "2,3p" $numbers
  assert closed == "2\n3\n", "a finished numeric range does not reopen"
}

test sed_write_command_writes_selected_records { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\n")?
  let out = fp"{file}.out"
  let app = fp"{ctx.core_dir}/sed.xsh"
  let script = "/[ac]/w " + out.display()
  let output = run.text ${ctx.xsh_bin} $app -- -n $script $file
  assert output == ""
  assert out.read_text()? == "a\nc\n"
  let stdout_copy = run.text ${ctx.xsh_bin} $app -- "1w /dev/stdout" $file
  assert stdout_copy == "a\na\nb\nc\n"
}

test sed_substitute_write_flag_writes_changed_records { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"qwe\nasd\n")?
  let out = fp"{file}.out"
  let app = fp"{ctx.core_dir}/sed.xsh"
  let script = "s/qwe/ZZZ/w " + out.display()
  let output = run.text ${ctx.xsh_bin} $app -- $script $file
  assert output == "ZZZ\nasd\n"
  assert out.read_text()? == "ZZZ\n"
}

test sed_write_commands_sharing_a_file_append_in_order { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\nb\nc\n")?
  let out = fp"{file}.out"
  let app = fp"{ctx.core_dir}/sed.xsh"
  let first = "/a/w " + out.display()
  let second = "/c/w " + out.display()
  let output = run.text ${ctx.xsh_bin} $app -- -n -e $first -e $second $file
  assert output == ""
  assert out.read_text()? == "a\nc\n"
  let unmatched = "/zzz/w " + out.display()
  let quiet = run.text ${ctx.xsh_bin} $app -- -n $unmatched $file
  assert quiet == ""
  assert out.read_text()? == "", "a write file is created even when no record is written"
}

test sed_nul_bytes_are_ordinary_text_for_regexes { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"\0woo\0woo\0")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let first = run.bytes ${ctx.xsh_bin} $app -- "s/woo/bang/" $file
  assert first == b"\0bang\0woo\0"
  let every = run.bytes ${ctx.xsh_bin} $app -- "s/woo/bang/g" $file
  assert every == b"\0bang\0bang\0"
  let anchored = run.bytes ${ctx.xsh_bin} $app -- -n "/^woo/p" $file
  assert anchored == b"", "a NUL ends the text the anchor can match at the start of"
  let dots = test.temp_file(ctx, name: "dots", contents: b"\0a\0")?
  let any = run.bytes ${ctx.xsh_bin} $app -- "s/./X/g" $dots
  assert any == b"XXX"
}

# Differential table. Each row of tests/data/sed/cases.jsonl runs the applet in
# a fresh copy of tests/data/sed/fixtures and its status, stdout, stderr, and
# the state of the files named in `show` must equal the transcript recorded in
# expected.txt, which tests/data/sed/regenerate.py produced from GNU sed 4.10.
type CaseRow = {name: Str, args: List[Str], stdin: Str, show: List[Str], env: List[Str], setup: List[Str], skip: Str}

pure escape_bytes(data: Bytes) -> Str {
  var out: List[Str] = []
  let digits = "0123456789abcdef"
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    if byte == 10 {
      out += ["\\n"]
    } else if byte == 92 {
      out += ["\\\\"]
    } else if byte == 39 {
      out += ["\\'"]
    } else if byte >= 32 and byte < 127 {
      out += [data[index..index + 1].utf8() ?? "?"]
    } else {
      out += [f"\\x{digits.byte_slice(byte / 16, 1)}{digits.byte_slice(byte % 16, 1)}"]
    }
  }
  out.join("")
}

pure octal_mode(mode: Int) -> Str {
  if mode < 8 { return f"{mode}" }
  f"{octal_mode(mode / 8)}{mode % 8}"
}

# The transcript `show` entry for one path: absent, directory, symlink target,
# or permission bits and contents.
proc describe_file(work: Path, name: Str) [fs, error] -> Result[Str] {
  let target = fp"{work}/{name}"
  let info = match fs.stat(target, follow_symlinks: false) {
    Ok(value) => value
    Err(_) => { return Ok(f"file {name} absent") }
  }
  if info.kind == "dir" { return Ok(f"file {name} directory") }
  if info.kind == "symlink" { return Ok(f"file {name} symlink '{escape_bytes(bytes.from_text(target.readlink()?.display()))}'") }
  let contents = target.read_bytes() ?? b""
  Ok(f"file {name} mode {octal_mode(info.mode % 4096)} '{escape_bytes(contents)}'")
}

# Fixture state every case starts from: links, a dangling link, an unreadable
# file, a read-only file, and a read-only directory.
proc prepare_work(ctx: TestContext, work: Path) [fs, error] -> Result[Unit] {
  let _ = fs.copy_tree(fp"{ctx.core_dir}/tests/data/sed/fixtures", work, overwrite: true)?
  fp"{work}/link.txt".symlink(to: p"a.txt")?
  fp"{work}/dangling".symlink(to: p"nowhere")?
  fp"{work}/dirlink".symlink(to: p"d")?
  fp"{work}/noaccess.txt".chmod(0o000)?
  fp"{work}/ro.txt".chmod(0o444)?
  fp"{work}/rodir".chmod(0o555)?
  Ok()
}

proc apply_setup(work: Path, command: Str) [fs, error] -> Result[Unit] {
  let words = command.split(" ")
  if words[0] == "mkdir" {
    fp"{work}/{words[1]}".mkdir()?
  } else if words[0] == "ln" {
    fp"{work}/{words[3]}".symlink(to: fp"{words[2]}")?
  } else if words[0] == "chmod" {
    var mode = 0
    for digit in words[1].split("") { mode = mode * 8 + (digit.parse_int() ?? 0) }
    fp"{work}/{words[2]}".chmod(mode)?
  } else {
    return Err(AssertionError.Failed(f"unknown setup command {command}"))
  }
  Ok()
}

proc transcript(ctx: TestContext, row: CaseRow, work: Path, scratch: Path) [fs, process, error] -> Result[Str] {
  prepare_work(ctx, work)?
  for command in row.setup { apply_setup(work, command)? }
  let out = fp"{scratch}/out"
  let err = fp"{scratch}/err"
  let input = if row.stdin == "" { p"/dev/null" } else { fp"{work}/{row.stdin}" }
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/sed.xsh".display(), "--"].extend(row.args)
  let plan = process.command_argv(ctx.xsh_bin, argv, work, {LC_ALL: "C"}, input, out, err, timeout: 60s)
  let status = process.run(plan)?
  var lines = [f"case {row.name}", f"status {status.exit_code()?}", f"stdout '{escape_bytes(out.read_bytes()?)}'", f"stderr '{escape_bytes(err.read_bytes()?)}'"]
  for name in row.show { lines += [describe_file(work, name)?] }
  Ok(lines.join("\n") + "\n")
}

# Restore permissions that would stop the scratch tree from being removed.
proc reset_work(work: Path) [fs] -> Unit {
  if work.exists() ?? false {
    let _ = fp"{work}/rodir".chmod(0o755)
    let _ = fp"{work}/noaccess.txt".chmod(0o644)
    let _ = work.remove()
  }
}

proc expected_transcripts(ctx: TestContext) [fs, error] -> Result[Map[Str]] {
  let text = fp"{ctx.core_dir}/tests/data/sed/expected.txt".read_text()?
  var table: Map[Str] = {}
  var name = ""
  var chunk: List[Str] = []
  for line in text.split("\n") {
    if line.starts_with("case ") {
      if name != "" { table = table.set(name, chunk.join("\n") + "\n") }
      name = line.byte_slice(5)
      chunk = [line]
    } else if line != "" {
      chunk += [line]
    }
  }
  if name != "" { table = table.set(name, chunk.join("\n") + "\n") }
  Ok(table)
}

# Run every row whose name starts with PREFIX and report the rows that differ.
proc check_group(ctx: TestContext, prefix: Str) [fs, process, error] -> Result[List[Str]] {
  let expected = expected_transcripts(ctx)?
  let scratch = test.temp_dir(ctx, name: "sed-table")?
  let work = fp"{scratch}/work"
  var failures: List[Str] = []
  var ran = 0
  for line in fp"{ctx.core_dir}/tests/data/sed/cases.jsonl".read_text()?.split("\n") {
    if line == "" { continue }
    let row = json.decode(line)?.require(CaseRow)?
    if !row.name.starts_with(prefix) or row.skip != "" { continue }
    reset_work(work)
    let actual = transcript(ctx, row, work, scratch)?
    ran += 1
    guard let want = expected.get(row.name) else {
      failures += [f"{row.name}: no expected transcript"]
      continue
    }
    if actual != want { failures += [f"{row.name} {row.args.join(" ")}:\n  expected:\n{want}  actual:\n{actual}"] }
  }
  reset_work(work)
  if ran == 0 { failures += [f"no cases ran for {prefix}"] }
  Ok(failures)
}

test sed_table_options_and_operands { |ctx|
  let failures = check_group(ctx, "opt-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_input_shapes { |ctx|
  let failures = check_group(ctx, "io-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_commands_over_inputs_a { |ctx|
  let failures = check_group(ctx, "cmd-0")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_commands_over_inputs_b { |ctx|
  let failures = check_group(ctx, "cmd-1")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_commands_over_inputs_c { |ctx|
  let failures = check_group(ctx, "cmd-2")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_commands_over_inputs_d { |ctx|
  let failures = check_group(ctx, "cmd-3")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_commands_over_inputs_e { |ctx|
  let failures = check_group(ctx, "cmd-4")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_commands_over_inputs_f { |ctx|
  let failures = check_group(ctx, "cmd-5")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_syntax_and_command_shapes { |ctx|
  let failures = check_group(ctx, "syn-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_addresses { |ctx|
  let failures = check_group(ctx, "addr-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_substitution_flags_and_replacements { |ctx|
  let failures = check_group(ctx, "sub-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_substitution_matching { |ctx|
  let failures = check_group(ctx, "sub2-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_extended_and_escapes { |ctx|
  let failures = check_group(ctx, "ere-")?
  assert failures.is_empty(), failures.join("\n")
  let escapes = check_group(ctx, "esc-")?
  assert escapes.is_empty(), escapes.join("\n")
  let queue = check_group(ctx, "queue-")?
  assert queue.is_empty(), queue.join("\n")
}

test sed_table_locale_bytes { |ctx|
  let failures = check_group(ctx, "loc-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_in_place { |ctx|
  let failures = check_group(ctx, "ip-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_shell_commands { |ctx|
  let failures = check_group(ctx, "eval-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_extra_cli_and_trailing_newline_shapes { |ctx|
  let options = check_group(ctx, "cli-")?
  assert options.is_empty(), options.join("\n")
  let tail = check_group(ctx, "tail-")?
  assert tail.is_empty(), tail.join("\n")
  let zero = check_group(ctx, "zero-")?
  assert zero.is_empty(), zero.join("\n")
}

test sed_table_separate_files_and_matching { |ctx|
  let separate = check_group(ctx, "sep-")?
  assert separate.is_empty(), separate.join("\n")
  let matching = check_group(ctx, "rx-")?
  assert matching.is_empty(), matching.join("\n")
  let mixed = check_group(ctx, "mix-")?
  assert mixed.is_empty(), mixed.join("\n")
}

test sed_table_passed_ranges { |ctx|
  let failures = check_group(ctx, "passed-")?
  assert failures.is_empty(), failures.join("\n")
}

test sed_table_syntax_corners_and_wrapping { |ctx|
  let corners = check_group(ctx, "corner-")?
  assert corners.is_empty(), corners.join("\n")
  let wrapping = check_group(ctx, "wrap-")?
  assert wrapping.is_empty(), wrapping.join("\n")
}

test sed_refused_options_fail_explicitly { |ctx|
  let file = test.temp_file(ctx, name: "input", contents: b"a\n")?
  let app = fp"{ctx.core_dir}/sed.xsh"
  let debug = run.capture --text ${ctx.xsh_bin} $app -- --debug p $file
  assert debug.status.exited_with(1)
  assert "--debug is not supported" in debug.stderr
  let posix = run.capture --text ${ctx.xsh_bin} $app -- --posix p $file
  assert posix.status.exited_with(1)
  assert "--posix is not supported" in posix.stderr
}

test sed_version_probe_line_matches_autoconf_check { |ctx|
  let app = fp"{ctx.core_dir}/sed.xsh"
  let version = run.text ${ctx.xsh_bin} $app -- --version
  assert version.starts_with("GNU sed version ")
}

test sed_table_exit_statuses_and_scripts { |ctx|
  let failures = check_group(ctx, "exit-")?
  assert failures.is_empty(), failures.join("\n")
  let scripts = check_group(ctx, "script-")?
  assert scripts.is_empty(), scripts.join("\n")
}

use lib.gnu

type Ran = {status: Int, stdout: Str, stderr: Str}

const WC_METADATA_SIZE = 2 * 1048576
const WC_I64_MAX = 9223372036854775807

# Keep byte-count edges covered when a filesystem rejects an i64-max sparse size.
proc wc_size_overlay(ctx: TestContext, file_size: Int, reported_size: Int) [fs, error] -> Result[Str] {
  let source = fp"{ctx.core_dir}/wc.xsh".read_text()?
  let header = "pure byte_count(size: Int) -> ByteCount {"
  let pieces = source.split(header)
  assert pieces.len() == 2, "wc byte_count header must be unique"
  let adapted = f"pure byte_count(file_size: Int) -> ByteCount {{\n  let size = if file_size == {file_size} {{ {reported_size} }} else {{ file_size }}"
  Ok(pieces[0] + adapted + pieces[1])
}

# Runs core/wc.xsh by its real path inside `dir`, with `input` as standard
# input, capturing both streams to files.
proc wc_in(
  ctx: TestContext,
  dir: Path,
  args: List[Str],
  input = b"",
  vars: Record = {LC_ALL: "C.UTF-8"},
) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "wc-run")?
  let stdin = test.temp_file(ctx, name: "wc-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/wc.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, dir, vars, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

# Preserve wc's diagnostic prefix by running the overlay from an exact wc.xsh path.
proc wc_overlay_in(ctx: TestContext, source: Str, dir: Path, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "wc-overlay")?
  let script = fp"{root}/wc.xsh"
  script.write(source)
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let modules = f"{ctx.core_dir}:{ctx.core_dir.parent()}"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, dir, {LC_ALL: "C.UTF-8", XSH_MODULE_PATH: modules}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc wc_run(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  wc_in(ctx, test.temp_dir(ctx, name: "wc-cwd")?, args, input)
}

proc wc_text(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Str] {
  let result = wc_run(ctx, args, input)?
  assert result.status == 0, f"wc {args.join(" ")}: {result.stderr}"
  Ok(result.stdout)
}

test test_wc_default_counts_and_stdin_width { |ctx|
  assert wc_text(ctx, [], b"one two\nthree\n")? == "      2       3      14\n", "standard input uses a width of 7"
  assert wc_text(ctx, ["-"], b"one two\nthree\n")? == "      2       3      14 -\n"
  assert wc_text(ctx, ["-l"], b"a\nb\n")? == "2\n", "a single count has width 1"
  assert wc_text(ctx, ["-c"], b"abc")? == "3\n"
}

test test_wc_count_selection_and_order { |ctx|
  assert wc_text(ctx, ["-lwmcL"], b"ab cd\nefg\n")? == "      2       3      10      10       5\n"
  assert wc_text(ctx, ["-c", "-l"], b"a\nb\n")? == "      2       4\n", "counts print in a fixed order"
  assert wc_text(ctx, ["--lines", "--words"], b"a b\n")? == "      1       2\n"
  assert wc_text(ctx, ["-ll", "-l"], b"a\n")? == "1\n", "repeated options are legal"
}

test test_wc_byte_only_stdin_counts_across_read_chunks { |ctx|
  let input = bytes.zero(131073)?
  assert wc_text(ctx, ["-c"], input)? == "131073\n"
}

test test_wc_characters_and_invalid_bytes { |ctx|
  assert wc_text(ctx, ["-mc"], bytes.from_text("héllo\n"))? == "      6       7\n"
  assert wc_text(ctx, [], b"a \xff b\n")? == "      1       3       6\n", "an invalid byte is a word character and no character"
  assert wc_text(ctx, ["-m"], b"a\xffb")? == "2\n"
  assert wc_text(ctx, ["-lc"], b"\xff\n\0x")? == "      1       4\n", "line and byte counts never decode"
}

test test_wc_words_follow_unicode_white_space_unless_posix { |ctx|
  let text = bytes.from_text("word\u{a0}word")
  assert wc_text(ctx, ["-w"], text)? == "2\n"
  assert wc_text(ctx, ["-w"], bytes.from_text("foo 💐 bar\n"))? == "3\n"
  assert wc_text(ctx, ["-w"], b"\x01\n")? == "1\n", "control characters are word characters"

  let strict = wc_in(ctx, test.temp_dir(ctx, name: "wc-cwd")?, ["-w"], text, {LC_ALL: "C", POSIXLY_CORRECT: "1"})?
  assert strict.stdout == "1\n"
}

test test_wc_max_line_length_is_display_width { |ctx|
  assert wc_text(ctx, ["-L"], b"\n123456")? == "6\n"
  assert wc_text(ctx, ["-L"], b"a\tb\n")? == "9\n", "a tab advances to the next multiple of 8"
  assert wc_text(ctx, ["-L"], b"abcdefgh\tb")? == "17\n"
  assert wc_text(ctx, ["-L"], bytes.from_text("日本語\n"))? == "6\n", "wide characters take two columns"
  assert wc_text(ctx, ["-L"], bytes.from_text("éx"))? == "2\n", "combining marks take none"
  assert wc_text(ctx, ["-L"], b"abc\rabcdef\x0cx")? == "6\n", "carriage returns and form feeds end a line"
}

test test_wc_files_width_follows_total_size { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-files")?
  fp"{dir}/small.txt".write("a b\n")
  fp"{dir}/big.txt".write_lines(["x" for n in range(600)])

  let result = wc_in(ctx, dir, ["small.txt", "big.txt"])?
  assert result.status == 0
  assert result.stdout == "   1    2    4 small.txt\n 600  600 1200 big.txt\n 601  602 1204 total\n", result.stdout

  assert wc_in(ctx, dir, ["-lw", "small.txt"])?.stdout == "1 2 small.txt\n"
  assert wc_in(ctx, dir, ["-c", "big.txt"])?.stdout == "1200 big.txt\n"
}

test test_wc_total_modes_and_abbreviations { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-total")?
  fp"{dir}/a".write("x y\n")
  fp"{dir}/b".write("z\n")

  assert wc_in(ctx, dir, ["a"])?.stdout == "1 2 4 a\n"
  assert wc_in(ctx, dir, ["a", "--total=always"])?.stdout == "1 2 4 a\n1 2 4 total\n"
  assert wc_in(ctx, dir, ["a", "b", "--total=never"])?.stdout == "1 2 4 a\n1 1 2 b\n"
  assert wc_in(ctx, dir, ["a", "b", "--total=only"])?.stdout == "2 3 6\n"
  assert wc_in(ctx, dir, ["a", "--tot=al"])?.stdout == "1 2 4 a\n1 2 4 total\n"
  assert wc_in(ctx, dir, ["--total=always", "--total=never", "a"])?.stdout == "1 2 4 a\n", "the last --total wins"

  let bad = wc_in(ctx, dir, ["--total=x", "a"])?
  assert bad.status == 1
  assert bad.stderr == "wc: invalid argument 'x' for '--total'\nValid arguments are:\n  - 'auto'\n  - 'always'\n  - 'only'\n  - 'never'\nTry 'wc --help' for more information.\n", bad.stderr

  let vague = wc_in(ctx, dir, ["--total=a", "a"])?
  assert vague.status == 1
  assert vague.stderr.starts_with("wc: ambiguous argument 'a' for '--total'\n"), vague.stderr
}

test test_wc_files0_from { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-files0")?
  fp"{dir}/a".write("x y\n")
  fp"{dir}/b".write("z\n")
  fp"{dir}/list".write(b"a\0b\0")

  let listed = wc_in(ctx, dir, ["--files0-from=list"])?
  assert listed.stdout == "1 2 4 a\n1 1 2 b\n2 3 6 total\n", listed.stdout

  let piped = wc_in(ctx, dir, ["--files0-from=-"], b"a\0b")?
  assert piped.stdout == "1 2 4 a\n1 1 2 b\n2 3 6 total\n"

  let both = wc_in(ctx, dir, ["--files0-from=list", "a"])?
  assert both.status == 1
  assert both.stdout == ""
  assert both.stderr == "wc: extra operand 'a'\nfile operands cannot be combined with --files0-from\nTry 'wc --help' for more information.\n", both.stderr

  let empty_names = wc_in(ctx, dir, ["--files0-from=-"], b"\0a\0\0")?
  assert empty_names.status == 1
  assert empty_names.stdout == "1 2 4 a\n1 2 4 total\n"
  assert empty_names.stderr == "wc: -:1: invalid zero-length file name\nwc: -:3: invalid zero-length file name\n", empty_names.stderr

  let dash = wc_in(ctx, dir, ["--files0-from=-"], b"-")?
  assert dash.status == 1
  assert dash.stderr == "wc: when reading file names from standard input, no file name of '-' allowed\n", dash.stderr

  let missing = wc_in(ctx, dir, ["--files0-from=nope"])?
  assert missing.status == 1
  assert missing.stderr == "wc: cannot open 'nope' for reading: No such file or directory\n", missing.stderr
}

test test_wc_files0_processes_each_name_before_list_eof { |ctx|
  let root = test.temp_dir(ctx, name: "wc-progressive-files0")?
  fp"{root}/a".write("one two\n")
  fp"{root}/b".write("x\n")
  let fifo = fp"{root}/names"
  fs.mkfifo(fifo, mode: 384)
  let wc_output = fp"{root}/wc-output"
  let wc_errors = fp"{root}/wc-errors"
  let output = fp"{root}/stdout"
  let errors = fp"{root}/stderr"
  let script = r"""set -eu
"$1" "$2" --files0-from=- < "$5" > "$6" 2> "$7" &
child=$!
{
  printf '%s\0' "$3"
  attempts=0
  while [ ! -s "$6" ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 100 ]; then
      kill -KILL "$child" 2>/dev/null || :
      wait "$child" 2>/dev/null || :
      echo "wc did not print a file count before list EOF" >&2
      exit 1
    fi
    sleep 0.01
  done
  printf '\0'
  attempts=0
  while [ ! -s "$7" ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 100 ]; then
      kill -KILL "$child" 2>/dev/null || :
      wait "$child" 2>/dev/null || :
      echo "wc did not print a list error before the next name" >&2
      exit 1
    fi
    sleep 0.01
  done
  printf '%s\0' "$4"
} > "$5"
child_status=0
wait "$child" || child_status=$?
cat "$6"
cat "$7" >&2
exit $child_status"""
  let plan = process.command_argv(
    "sh",
    [
      "sh",
      "-c",
      script,
      "sh",
      ctx.xsh_bin,
      fp"{ctx.core_dir}/wc.xsh",
      "a",
      "b",
      fifo,
      wc_output,
      wc_errors,
    ],
    cwd: root,
    stdout: output,
    stderr: errors,
    timeout: 5s,
  )
  let status = process.run(plan)?

  assert status.exited_with(1), errors.read_text()?
  assert output.read_text()? == "1 2 8 a\n1 1 2 b\n2 3 10 total\n"
  assert errors.read_text()? == "wc: -:2: invalid zero-length file name\n"
}

test test_wc_files0_stops_after_stdout_write_error { |ctx|
  guard p"/dev/full".exists() else { test.skip("requires /dev/full"); return }

  let root = test.temp_dir(ctx, name: "wc-files0-write-error")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = r"""printf '%s\0' /dev/null /dev/null | "$1" "$2" --files0-from=- --total=never > /dev/full"""
  let plan = process.command_argv(
    "sh",
    [
      "sh",
      "-c",
      script,
      "sh",
      ctx.xsh_bin,
      fp"{ctx.core_dir}/wc.xsh",
    ],
    cwd: root,
    stdout:,
    stderr:,
    timeout: 5s,
  )
  let status = process.run(plan)?

  assert status.exited_with(1), stderr.read_text()?
  assert stderr.read_text()? == "wc: write error: No space left on device\n"
}

test test_wc_errors_are_reported_per_input_and_set_the_status { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-errors")?
  fp"{dir}/a".write("x\n")
  fp"{dir}/sub".mkdir()

  let result = wc_in(ctx, dir, ["a", "missing", "sub", "a"])?
  assert result.status == 1
  assert result.stdout == "      1       1       2 a\n      0       0       0 sub\n      1       1       2 a\n      2       2       4 total\n", result.stdout
  assert result.stderr == "wc: missing: No such file or directory\nwc: sub: Is a directory\n", result.stderr

  let quoted = wc_in(ctx, dir, ["no such file"])?
  assert quoted.stderr == "wc: 'no such file': No such file or directory\n", quoted.stderr
}

test test_wc_names_with_newlines_are_quoted_in_the_output { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-quote")?
  fp"{dir}/12\n34.txt".write("")

  assert wc_in(ctx, dir, ["12\n34.txt"])?.stdout == "0 0 0 '12'$'\\n''34.txt'\n"
}

test test_wc_debug_reports_scalar_line_counting { |ctx|
  let result = wc_run(ctx, ["--debug"])?
  assert result.status == 0
  assert result.stderr == "wc: avx512 support not detected\nwc: avx2 support not detected\n", result.stderr
}

test test_wc_help_and_version { |ctx|
  let help = wc_run(ctx, ["--help"])?
  assert help.status == 0
  assert "Usage: wc [OPTION]... [FILE]..." in help.stdout

  let version = wc_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("wc")

  let unknown = wc_run(ctx, ["-q"])?
  assert unknown.status == 1
  assert unknown.stderr == "wc: invalid option -- 'q'\nTry 'wc --help' for more information.\n", unknown.stderr
}

test test_wc_c_locale_counts_bytes_and_byte_words { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-c-locale")?
  let input = bytes.from_text("a\u{2003}b\u{a0}c\u{2060}d\n")
  let result = wc_in(ctx, dir, ["-wmcL"], input, {LC_ALL: "C"})?
  assert result.status == 0, result.stderr
  assert result.stdout == "      3      13      13       4\n", result.stdout
  let strict = wc_in(ctx, dir, ["-w"], input, {LC_ALL: "C", POSIXLY_CORRECT: ""})?
  assert strict.stdout == "1\n", strict.stdout
}

test test_wc_utf8_word_joiner_and_posix_whitespace { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-utf8-words")?
  let input = bytes.from_text("a\u{2060}b\u{2003}c")
  let result = wc_in(ctx, dir, ["-w"], input)?
  assert result.stdout == "3\n", result.stdout
  assert wc_in(ctx, dir, ["-L"], bytes.from_text("a\u{2060}b"))?.stdout == "2\n"
  let strict = wc_in(ctx, dir, ["-w"], input, {LC_ALL: "C.UTF-8", POSIXLY_CORRECT: "1"})?
  assert strict.stdout == "2\n", strict.stdout
}

test test_wc_sparse_totals_and_alignment_overflow { |ctx|
  let dir = test.temp_dir(ctx, name: "wc-sparse")?
  let file = fp"{dir}/a"
  file.write("")
  file.truncate(WC_METADATA_SIZE)
  let moderate = wc_in(ctx, dir, ["-c", "a", "a"])?
  assert moderate.status == 0, moderate.stderr
  assert moderate.stdout == "2097152 a\n2097152 a\n4194304 total\n", moderate.stdout

  let oversized = file.truncate(WC_I64_MAX)
  match oversized {
    Ok(_) => {
      let doubled = wc_in(ctx, dir, ["-c", "a", "a"])?
      assert doubled.status == 0, doubled.stderr
      assert doubled.stdout == " 9223372036854775807 a\n 9223372036854775807 a\n18446744073709551614 total\n", doubled.stdout
      let overflow = wc_in(ctx, dir, ["-c", "a", "a", "a"])?
      assert overflow.status == 1
      assert overflow.stdout == " 9223372036854775807 a\n 9223372036854775807 a\n 9223372036854775807 a\n18446744073709551615 total\n", overflow.stdout
      assert overflow.stderr == "wc: total bytes: Value too large for defined data type\n", overflow.stderr
      let hidden = wc_in(ctx, dir, ["-c", "--total=never", "a", "a", "a"])?
      assert hidden.status == 0, hidden.stderr
      assert hidden.stdout == " 9223372036854775807 a\n 9223372036854775807 a\n 9223372036854775807 a\n", hidden.stdout
    }
    Err(failure) => {
      guard gnu.errno(failure) == 27 else { return Err(failure) }
      file.truncate(WC_METADATA_SIZE)
      let source = wc_size_overlay(ctx, WC_METADATA_SIZE, WC_I64_MAX)?
      let title = file.display()
      let item = f" 9223372036854775807 {title}\n"
      let doubled = wc_overlay_in(ctx, source, dir, ["-c", title, title])?
      assert doubled.status == 0, doubled.stderr
      assert doubled.stdout == item + item + "18446744073709551614 total\n", doubled.stdout
      let overflow = wc_overlay_in(ctx, source, dir, ["-c", title, title, title])?
      assert overflow.status == 1
      assert overflow.stdout == item + item + item + "18446744073709551615 total\n", overflow.stdout
      assert overflow.stderr == "wc: total bytes: Value too large for defined data type\n", overflow.stderr
      let hidden = wc_overlay_in(ctx, source, dir, ["--total=never", "-c", title, title, title])?
      assert hidden.status == 0, hidden.stderr
      assert hidden.stdout == item + item + item, hidden.stdout
      assert hidden.stderr == "", hidden.stderr
    }
  }
}

test test_wc_byte_count_u64_arithmetic_and_format { |ctx|
  let source = fp"{ctx.core_dir}/wc.xsh".read_text()?
  let assertions = r"""
let one_file = Counts(lines: 0, words: 0, chars: 0, bytes: byte_count(9223372036854775807), longest: 0)
assert byte_text(one_file.bytes) == "9223372036854775807"
let two_files = add_counts(one_file, one_file)
assert byte_text(two_files.bytes) == "18446744073709551614" and ! two_files.bytes.overflow
let exact_max = add_counts(two_files, Counts(lines: 0, words: 0, chars: 0, bytes: byte_count(1), longest: 0))
assert byte_text(exact_max.bytes) == "18446744073709551615" and ! exact_max.bytes.overflow
let over_max = add_counts(exact_max, Counts(lines: 0, words: 0, chars: 0, bytes: byte_count(1), longest: 0))
assert byte_text(over_max.bytes) == "18446744073709551615" and over_max.bytes.overflow
let shown = Shown(lines: false, words: false, chars: false, bytes: true, longest: false)
assert line_for(one_file, shown, 20, "a") == " 9223372036854775807 a\n"
assert line_for(over_max, shown, 20, "total") == "18446744073709551615 total\n"
"""
  let result = test.run_xsh(
    ctx,
    source + assertions,
    [],
    ["--version"],
    env: {XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}"},
    name: "wc.xsh",
  )?
  assert result.success, result.stderr
  assert result.stdout.starts_with("wc"), result.stdout
  assert result.stderr == ""
}

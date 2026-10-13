##! Transcribed behavior from the MIT-licensed uutils sort integration tests.
use support.uu as uu

proc fixture_bytes(ctx: TestContext, name: Str) [fs, error] -> Result[Bytes, Error] {
  Ok(fp"{ctx.core_dir}/tests/data/uutils/sort/{name}".read_bytes()?)
}

proc fixture_case(ctx: TestContext, base: Str, options: List[Str]) [fs, process, env, error] {
  for option in options {
    for debug in [false, true] {
      let s = uu.scene(ctx)?
      let input = f"{base}.txt"
      uu.fixture(s, "sort", input, input)
      let args = [input].extend(if debug { ["--debug"] } else { [] }).extend([word for word in option.split(" ") if word != ""])
      let r = uu.invoke(s, "sort", args)?
      uu.succeeds(r)
      let expected = if debug { f"{base}.expected.debug" } else { f"{base}.expected" }
      uu.stdout_is_bytes(r, fixture_bytes(ctx, expected)?)
    }
  }
}

# Set both child limits before executing the applet; the runner keeps its own limits.
proc limited(s: uu.Scene, args: List[Str], count: Int) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let argv = uu.argv(s, "sort", [Path(word) for word in args])?
  let words = [p"/bin/sh", p"-c", Path(r"""ulimit -Sn "$1"; ulimit -Hn "$1"; shift; exec "$@" """), p"sort-limit", Path(f"{count}")].extend(argv)
  let out = uu.at(s, ".limited-out")
  let err = uu.at(s, ".limited-err")
  let plan = process.command_argv(p"/bin/sh", words, s.root, {}, b"", out, err)
  let status = process.run(plan)?.exit_code()?
  Ok({util: "sort", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

pure padded(number: Int, width: Int) -> Str {
  let value = f"{number}"
  "000000".byte_slice(0, length: width - value.byte_len()) + value
}

# Match the upstream charmap probe, including its UTF-8 output criterion.
proc locale_available(s: uu.Scene, locale: Str) [fs, process, error] -> Result[Bool, Error] {
  match process.which("locale") { Err(_) => return Ok(false), Ok(_) => {} }
  let out = uu.at(s, ".locale-out")
  let err = uu.at(s, ".locale-err")
  let plan = process.command_argv(process.which("locale")?, [p"locale", p"charmap"], s.root, {LC_ALL: locale}, b"", out, err)
  let status = process.run(plan)?
  Ok(out.read_text()?.trim() == "UTF-8")
}

# origin: uutils test_sort::test_keys_invalid_field_option
test test_uu_sort_keys_invalid_field_option { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "1.1x"], stdin: bytes.from_text(""), vars: {})?
  uu.fails(r)
  uu.stderr_only(r, "sort: stray character in field spec: invalid field specification '1.1x'\n")
  }
}

# origin: uutils test_sort::test_keys_invalid_field_zero
test test_uu_sort_keys_invalid_field_zero { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "0.1"], stdin: bytes.from_text(""), vars: {})?
  uu.fails(r)
  uu.stderr_only(r, "sort: field number is zero: invalid field specification '0.1'\n")
  }
}

# origin: uutils test_sort::test_keys_invalid_number_formats
test test_uu_sort_keys_invalid_number_formats { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "0"], stdin: bytes.from_text(""), vars: {})?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: field number is zero: invalid field specification '0'\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "2.,3"], stdin: bytes.from_text(""), vars: {})?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: invalid number after '.': invalid count at start of ',3'\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "2,"], stdin: bytes.from_text(""), vars: {})?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: invalid number after ',': invalid count at start of ''\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "1.1,-k0"], stdin: bytes.from_text(""), vars: {})?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: invalid number after ',': invalid count at start of '-k0'\n")
  }
}

# origin: uutils test_sort::test_keys_multiple_ranges
test test_uu_sort_keys_multiple_ranges { |ctx|
  fixture_case(ctx, "keys_multiple_ranges", ["-k 2,2 -k 3,3"])
}

# origin: uutils test_sort::test_keys_negative_size_match
test test_uu_sort_keys_negative_size_match { |ctx|
  fixture_case(ctx, "keys_negative_size", ["-k 3,1"])
}

# origin: uutils test_sort::test_keys_no_char_match
test test_uu_sort_keys_no_char_match { |ctx|
  fixture_case(ctx, "keys_no_char_match", ["-k 1.2"])
}

# origin: uutils test_sort::test_keys_no_field_match
test test_uu_sort_keys_no_field_match { |ctx|
  fixture_case(ctx, "keys_no_field_match", ["-k 4,4"])
}

# origin: uutils test_sort::test_keys_open_ended
test test_uu_sort_keys_open_ended { |ctx|
  fixture_case(ctx, "keys_open_ended", ["-k 2.3"])
}

# origin: uutils test_sort::test_keys_stable
test test_uu_sort_keys_stable { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "1,2.1", "-s"], stdin: bytes.from_text("a  b\na b\na   b\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_only(r, "a  b\na b\na   b\n")
  }
}

# origin: uutils test_sort::test_keys_with_options
test test_uu_sort_keys_with_options { |ctx|
  for args in [["-k", "2,2n"], ["-k", "2n,2"], ["-k", "2,2", "-n"]] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", args, stdin: bytes.from_text("aa 3 cc\ndd 1 ff\ngg 2 cc\n"))?
    uu.succeeds(r)
    uu.stdout_only(r, "dd 1 ff\ngg 2 cc\naa 3 cc\n")
  }
}

# origin: uutils test_sort::test_keys_with_options_blanks_end
test test_uu_sort_keys_with_options_blanks_end { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k", "1,2.1b", "-s"], stdin: bytes.from_text("a  b\na b\na   b\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_only(r, "a   b\na  b\na b\n")
  }
}

# origin: uutils test_sort::test_keys_with_options_blanks_start
test test_uu_sort_keys_with_options_blanks_start { |ctx|
  for args in [["-k", "2b,2"], ["-k", "2,2", "-b"]] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", args, stdin: bytes.from_text("aa   3 cc\ndd  1 ff\ngg         2 cc\n"))?
    uu.succeeds(r)
    uu.stdout_only(r, "dd  1 ff\ngg         2 cc\naa   3 cc\n")
  }
}

# origin: uutils test_sort::test_legacy_plus_minus_accepts_when_modern_posix2
test test_uu_sort_legacy_plus_minus_accepts_when_modern_posix2 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input.txt", "aa\nbb\n")
  let r = uu.invoke(s, "sort", ["+0.18446744073709551615R", "input.txt"], vars: {_POSIX2_VERSION: "200809"})?
  uu.succeeds(r)
  uu.stdout_is(r, "aa\nbb\n")
}

# origin: uutils test_sort::test_legacy_plus_minus_accepts_with_size_max
test test_uu_sort_legacy_plus_minus_accepts_with_size_max { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input.txt", "aa\nbb\n")
  let r = uu.invoke(s, "sort", ["+1", "-1.18446744073709551615R", "input.txt"], vars: {_POSIX2_VERSION: "200809"})?
  uu.succeeds(r)
  uu.stdout_is(r, "aa\nbb\n")
}

# origin: uutils test_sort::test_locale_c_byte_order
test test_uu_sort_locale_c_byte_order { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", [], stdin: bytes.from_text("a\nA\nb\nB\n"), vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "A\nB\na\nb\n")
  }
}

# origin: uutils test_sort::test_locale_collation_c_locale
test test_uu_sort_locale_collation_c_locale { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", [], stdin: bytes.from_text("é\ne\nE\na\nA\nz\n"), vars: {LC_ALL: "C"})?
  uu.succeeds(r)
  uu.stdout_is(r, "A\nE\na\ne\nz\né\n")
  }
}

# origin: uutils test_sort::test_locale_collation_utf8
test test_uu_sort_locale_collation_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", [], stdin: bytes.from_text("z\né\ne\na\n"), vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.trim().split("\n")
  assert lines.len() == 4
  assert lines[0] == "a"
  if lines[3] == "é" {
    assert lines == ["a", "e", "z", "é"]
  } else {
    assert lines[3] == "z"
    assert "e" in [lines[0], lines[1], lines[2]]
    assert "é" in [lines[0], lines[1], lines[2]]
  }
}

# origin: uutils test_sort::test_locale_posix_byte_order
test test_uu_sort_locale_posix_byte_order { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", [], stdin: bytes.from_text("a\nA\nb\nB\n"), vars: {LC_ALL: "POSIX"})?
  uu.succeeds(r)
  uu.stdout_is(r, "A\nB\na\nb\n")
  }
}

# origin: uutils test_sort::test_locale_utf8_with_key_field
test test_uu_sort_locale_utf8_with_key_field { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-k3"], stdin: bytes.from_text("a b 5433 down data path1 path2 path3 path4 path5\nc d 5435 down data path1 path2 path3 path4 path5\ne f 5436 down data path1 path2 path3 path4 path5\n"), vars: {LANG: "en_US.utf8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "a b 5433 down data path1 path2 path3 path4 path5\nc d 5435 down data path1 path2 path3 path4 path5\ne f 5436 down data path1 path2 path3 path4 path5\n")
  }
}

# origin: uutils test_sort::test_locale_with_ignore_case_flag
test test_uu_sort_locale_with_ignore_case_flag { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-f"], stdin: b"a\nA\nb\nB\n", vars: {LC_ALL: "en_US.UTF-8"})?
  uu.succeeds(r)
  let lines = r.stdout.utf8()?.trim().split("\n")
  assert lines.len() == 4
  for a in range(4) {
    for b in range(4) {
      if lines[a] in ["a", "A"] and lines[b] in ["b", "B"] { assert a < b }
    }
  }
}

# origin: uutils test_sort::test_merge_batch_size
test test_uu_sort_merge_batch_size { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_1.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_2.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt")
  let r = uu.invoke(s, "sort", ["--batch-size=2", "-m", "--unique", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_1.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "merge_ints_interleaved.expected")?)
  }
}

# origin: uutils test_sort::test_merge_batch_size_with_limit
test test_uu_sort_merge_batch_size_with_limit { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_1.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_2.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt")
  let r = limited(s, ["--batch-size=2", "-m", "--unique", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_1.txt"], 7)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "merge_ints_interleaved.expected")?)
  }
}

# origin: uutils test_sort::test_merge_batches
test test_uu_sort_merge_batches { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "ext_sort.txt", "ext_sort.txt")
  let r = uu.invoke(s, "sort", ["ext_sort.txt", "-n", "-S", "150b"], stdin: bytes.from_text(""), vars: {}, timeout: 120s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "ext_sort.expected")?)
  }
}

# origin: uutils test_sort::test_merge_empty_input
test test_uu_sort_merge_empty_input { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "empty.txt", "empty.txt")
  let r = uu.invoke(s, "sort", ["-m", "empty.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.no_output(r)
  }
}

# origin: uutils test_sort::test_merge_flush_error_is_reported
test test_uu_sort_merge_flush_error_is_reported { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input.txt", "line\n")
  let r = uu.invoke(s, "sort", ["-m", "input.txt"], stdout: p"/dev/full")?
  uu.fails(r)
  uu.stderr_is(r, "sort: fflush failed: 'standard output': No space left on device\nsort: write error\n")
}

# origin: uutils test_sort::test_merge_interleaved
test test_uu_sort_merge_interleaved { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_1.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_2.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt")
  let r = uu.invoke(s, "sort", ["-m", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_3.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "merge_ints_interleaved.expected")?)
  }
}

# origin: uutils test_sort::test_merge_more_files_than_fd_limit
test test_uu_sort_merge_more_files_than_fd_limit { |ctx|
  let s = uu.scene(ctx)?
  let names = [f"fdlimit_{padded(i, 2)}.txt" for i in range(40)]
  for i in range(40) { uu.write(s, names[i], f"{padded(i, 2)}\n") }
  let expected = [padded(i, 2) for i in range(40)].join("\n") + "\n"
  let args = ["-m"].extend(names)
  let r = limited(s, args, 24)?
  uu.succeeds(r)
  uu.stdout_only(r, expected)
}

# origin: uutils test_sort::test_merge_output_file_is_truncated
test test_uu_sort_merge_output_file_is_truncated { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "merged", "zzz-leftover-one\nzzz-leftover-two\nzzz-leftover-three\n")
  uu.write(s, "pears", "damson\nquince\n")
  let r = uu.invoke(s, "sort", ["-m", "-o", "merged", "pears"], stdin: bytes.from_text(""))?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "merged", "damson\nquince\n")
}

# origin: uutils test_sort::test_merge_preserves_long_lines
test test_uu_sort_merge_preserves_long_lines { |ctx|
  let s = uu.scene(ctx)?
  let input = [[value for i in range(32000)].join("") for value in ["a", "b", "c"]].join("\n") + "\n"
  uu.write(s, "long-lines.txt", input)
  let r = uu.invoke(s, "sort", ["-m", "long-lines.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert r.stdout.utf8()?.split("\n").len() - 1 == 3
  assert r.stdout.len() == input.byte_len()
  uu.stdout_is_bytes(r, bytes.from_text(input))
}

# origin: uutils test_sort::test_merge_reversed
test test_uu_sort_merge_reversed { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_ints_reversed_1.txt", "merge_ints_reversed_1.txt")
  uu.fixture(s, "sort", "merge_ints_reversed_2.txt", "merge_ints_reversed_2.txt")
  uu.fixture(s, "sort", "merge_ints_reversed_3.txt", "merge_ints_reversed_3.txt")
  let r = uu.invoke(s, "sort", ["-m", "--reverse", "merge_ints_reversed_1.txt", "merge_ints_reversed_2.txt", "merge_ints_reversed_3.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "merge_ints_reversed.expected")?)
  }
}

# origin: uutils test_sort::test_merge_stable
test test_uu_sort_merge_stable { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_stable_1.txt", "merge_stable_1.txt")
  uu.fixture(s, "sort", "merge_stable_2.txt", "merge_stable_2.txt")
  let r = uu.invoke(s, "sort", ["-m", "--stable", "-n", "merge_stable_1.txt", "merge_stable_2.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "merge_stable.expected")?)
  }
}

# origin: uutils test_sort::test_merge_unique
test test_uu_sort_merge_unique { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_1.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_2.txt")
  uu.fixture(s, "sort", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt")
  let r = uu.invoke(s, "sort", ["-m", "--unique", "merge_ints_interleaved_1.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_3.txt", "merge_ints_interleaved_2.txt", "merge_ints_interleaved_1.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "merge_ints_interleaved.expected")?)
  }
}

# origin: uutils test_sort::test_merge_write_error_does_not_panic
test test_uu_sort_merge_write_error_does_not_panic { |ctx|
  let s = uu.scene(ctx)?
  let input = [padded(line, 6) for line in range(100000)].join("\n") + "\n"
  uu.write(s, "a.txt", input)
  uu.write(s, "b.txt", input)
  for i in range(20) {
    let r = uu.invoke(s, "sort", ["-m", "a.txt", "b.txt"], stdout: p"/dev/full", timeout: 30s)?
    uu.fails(r)
    uu.fails_with_code(r, 2)
    uu.stderr_contains(r, "No space left on device")
    assert !("panicked" in r.stderr as Str)
  }
}

# origin: uutils test_sort::test_mixed_floats_ints_chars_numeric
test test_uu_sort_mixed_floats_ints_chars_numeric { |ctx|
  fixture_case(ctx, "mixed_floats_ints_chars_numeric", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_mixed_floats_ints_chars_numeric_reverse
test test_uu_sort_mixed_floats_ints_chars_numeric_reverse { |ctx|
  fixture_case(ctx, "mixed_floats_ints_chars_numeric_unique_reverse", ["-nur"])
}

# origin: uutils test_sort::test_mixed_floats_ints_chars_numeric_stable
test test_uu_sort_mixed_floats_ints_chars_numeric_stable { |ctx|
  fixture_case(ctx, "mixed_floats_ints_chars_numeric_stable", ["-ns"])
}

# origin: uutils test_sort::test_mixed_floats_ints_chars_numeric_unique
test test_uu_sort_mixed_floats_ints_chars_numeric_unique { |ctx|
  fixture_case(ctx, "mixed_floats_ints_chars_numeric_unique", ["-nu"])
}

# origin: uutils test_sort::test_month_default
test test_uu_sort_month_default { |ctx|
  fixture_case(ctx, "month_default", ["-M", "--month-sort", "--sort=month"])
}

# origin: uutils test_sort::test_month_default2
test test_uu_sort_month_default2 { |ctx|
  for args in [["-M"], ["--month-sort"], ["--sort=month"]] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", args, stdin: bytes.from_text("JAn\nMAY\n000may\nJun\nFeb"))?
    uu.succeeds(r)
    uu.stdout_only(r, "000may\nJAn\nFeb\nMAY\nJun\n")
  }
}

# origin: uutils test_sort::test_month_sort_french_embedded_blanks
test test_uu_sort_month_sort_french_embedded_blanks { |ctx|
  let s = uu.scene(ctx)?
  if !locale_available(s, "fr_FR.UTF-8") { test.skip("requested UTF-8 locale unavailable") }
  let r = uu.invoke(s, "sort", ["-M"], stdin: bytes.from_text("juin\nav   ril\nmars\n"), vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "av   ril\nmars\njuin\n")
}

# origin: uutils test_sort::test_month_sort_french_locale
test test_uu_sort_month_sort_french_locale { |ctx|
  let s = uu.scene(ctx)?
  if !locale_available(s, "fr_FR.UTF-8") { test.skip("requested UTF-8 locale unavailable") }
  let r = uu.invoke(s, "sort", ["-M"], stdin: bytes.from_text("mai\ndéc.\njanv.\njuin\nfévr.\nmars\navril\njuil.\naoût\nsept.\noct.\nnov.\n"), vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "janv.\nfévr.\nmars\navril\nmai\njuin\njuil.\naoût\nsept.\noct.\nnov.\ndéc.\n")
}

# origin: uutils test_sort::test_month_sort_hungarian_locale
test test_uu_sort_month_sort_hungarian_locale { |ctx|
  let s = uu.scene(ctx)?
  if !locale_available(s, "hu_HU.UTF-8") { test.skip("requested UTF-8 locale unavailable") }
  let r = uu.invoke(s, "sort", ["-M"], stdin: bytes.from_text("máj\ndec\njan\njún\nfebr\nmárc\nápr\njúl\naug\nszept\nokt\nnov\n"), vars: {LC_ALL: "hu_HU.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "jan\nfebr\nmárc\nápr\nmáj\njún\njúl\naug\nszept\nokt\nnov\ndec\n")
}

# origin: uutils test_sort::test_month_sort_japanese_locale
test test_uu_sort_month_sort_japanese_locale { |ctx|
  let s = uu.scene(ctx)?
  if !locale_available(s, "ja_JP.UTF-8") { test.skip("requested UTF-8 locale unavailable") }
  let r = uu.invoke(s, "sort", ["-M"], stdin: bytes.from_text("5月\n12月\n1月\n6月\n2月\n3月\n4月\n7月\n8月\n9月\n10月\n11月\n"), vars: {LC_ALL: "ja_JP.UTF-8"})?
  uu.succeeds(r)
  uu.stdout_is(r, "1月\n2月\n3月\n4月\n5月\n6月\n7月\n8月\n9月\n10月\n11月\n12月\n")
}

# origin: uutils test_sort::test_month_stable
test test_uu_sort_month_stable { |ctx|
  fixture_case(ctx, "month_stable", ["-Ms"])
}

# origin: uutils test_sort::test_months_dedup
test test_uu_sort_months_dedup { |ctx|
  fixture_case(ctx, "months-dedup", ["-Mu"])
}

# origin: uutils test_sort::test_months_whitespace
test test_uu_sort_months_whitespace { |ctx|
  fixture_case(ctx, "months-whitespace", ["-M", "--month-sort", "--sort=month", "--sort=mont", "--sort=m"])
}

# origin: uutils test_sort::test_more_files_than_fd_limit
test test_uu_sort_more_files_than_fd_limit { |ctx|
  let s = uu.scene(ctx)?
  let names = [f"fdlimit_{padded(i, 2)}.txt" for i in range(40)]
  for i in range(40) { uu.write(s, names[i], f"{padded(39 - i, 2)}\n") }
  let expected = [padded(i, 2) for i in range(40)].join("\n") + "\n"
  let args = [].extend(names)
  let r = limited(s, args, 24)?
  uu.succeeds(r)
  uu.stdout_only(r, expected)
}

# origin: uutils test_sort::test_multiple_decimals_general
test test_uu_sort_multiple_decimals_general { |ctx|
  fixture_case(ctx, "multiple_decimals_general", ["-g", "--general-numeric-sort", "--sort=general-numeric", "--sort=general-numeri", "--sort=general", "--sort=g"])
}

# origin: uutils test_sort::test_multiple_decimals_numeric
test test_uu_sort_multiple_decimals_numeric { |ctx|
  fixture_case(ctx, "multiple_decimals_numeric", ["-n", "--numeric-sort", "--sort=numeric", "--sort=n"])
}

# origin: uutils test_sort::test_multiple_files
test test_uu_sort_multiple_files { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "sort", "multiple_files1.txt", "multiple_files1.txt")
  uu.fixture(s, "sort", "multiple_files2.txt", "multiple_files2.txt")
  let r = uu.invoke(s, "sort", ["-n", "multiple_files1.txt", "multiple_files2.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, fixture_bytes(ctx, "multiple_files.expected")?)
  }
}

# origin: uutils test_sort::test_multiple_groupings_numeric
test test_uu_sort_multiple_groupings_numeric { |ctx|
  fixture_case(ctx, "multiple_groupings_numeric", ["-n", "--numeric-sort", "--sort=numeric", "--sort=n"])
}

# origin: uutils test_sort::test_no_error_for_version
test test_uu_sort_no_error_for_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["--version"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "sort")
}

# origin: uutils test_sort::test_non_printing_chars
test test_uu_sort_non_printing_chars { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-i"], stdin: bytes.from_text("a👦🏻aa\naaaa"), vars: {})?
  uu.succeeds(r)
  uu.stdout_only(r, "a👦🏻aa\naaaa\n")
  }
}

# origin: uutils test_sort::test_nonexistent_file
test test_uu_sort_nonexistent_file { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["nonexistent.txt"], stdin: bytes.from_text(""), vars: {})?
  uu.fails_with_code(r, 2)
  uu.stderr_only(r, "sort: cannot read: nonexistent.txt: No such file or directory\n")
  }
}

# origin: uutils test_sort::test_numeric_fixed_floats
test test_uu_sort_numeric_fixed_floats { |ctx|
  fixture_case(ctx, "numeric_fixed_floats", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_floats
test test_uu_sort_numeric_floats { |ctx|
  fixture_case(ctx, "numeric_floats", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_floats2
test test_uu_sort_numeric_floats2 { |ctx|
  for args in [["-n"], ["--numeric-sort"]] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", args, stdin: bytes.from_text("1.444\n8.013\n1.58590\n-8.90880\n1.040000000\n-.05"))?
    uu.succeeds(r)
    uu.stdout_only(r, "-8.90880\n-.05\n1.040000000\n1.444\n1.58590\n8.013\n")
  }
}

# origin: uutils test_sort::test_numeric_floats_and_ints
test test_uu_sort_numeric_floats_and_ints { |ctx|
  fixture_case(ctx, "numeric_floats_and_ints", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_floats_and_ints2
test test_uu_sort_numeric_floats_and_ints2 { |ctx|
  for args in [["-n"], ["--numeric-sort"]] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", args, stdin: bytes.from_text("1.444\n8.013\n1\n-8\n1.04\n-1"))?
    uu.succeeds(r)
    uu.stdout_only(r, "-8\n-1\n1\n1.04\n1.444\n8.013\n")
  }
}

# origin: uutils test_sort::test_numeric_floats_with_nan
test test_uu_sort_numeric_floats_with_nan { |ctx|
  fixture_case(ctx, "numeric_floats_with_nan", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_floats_with_nan2
test test_uu_sort_numeric_floats_with_nan2 { |ctx|
  fixture_case(ctx, "numeric-floats-with-nan2", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_sort_rejects_leading_plus_sign
test test_uu_sort_numeric_sort_rejects_leading_plus_sign { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n"], stdin: bytes.from_text("+1\n+10\n+2\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "+1\n+10\n+2\n")
  }
}

# origin: uutils test_sort::test_numeric_sort_uses_the_key_not_the_whole_line
test test_uu_sort_numeric_sort_uses_the_key_not_the_whole_line { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n", "-k1.2"], stdin: bytes.from_text("19\n21\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "21\n19\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n", "-t.", "-k2"], stdin: bytes.from_text("1.9\n2.1\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "2.1\n1.9\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n", "-k1r"], stdin: bytes.from_text("19\n21\n3\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "3\n21\n19\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n", "-k1.2"], stdin: bytes.from_text("1\n10\n2\n20\n3\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "1\n10\n2\n20\n3\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n"], stdin: bytes.from_text("19\n21\n3\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "3\n19\n21\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-n", "-k1"], stdin: bytes.from_text("19\n21\n3\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "3\n19\n21\n")
  }
}

# origin: uutils test_sort::test_numeric_unfixed_floats
test test_uu_sort_numeric_unfixed_floats { |ctx|
  fixture_case(ctx, "numeric_unfixed_floats", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_unique
test test_uu_sort_numeric_unique { |ctx|
  fixture_case(ctx, "numeric_unique", ["-nu"])
}

# origin: uutils test_sort::test_numeric_unique_ints
test test_uu_sort_numeric_unique_ints { |ctx|
  fixture_case(ctx, "numeric_unsorted_ints_unique", ["-nu"])
}

# origin: uutils test_sort::test_numeric_unique_ints2
test test_uu_sort_numeric_unique_ints2 { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-nu"], stdin: bytes.from_text("9\n9\n8\n1\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_only(r, "1\n8\n9\n")
  }
}

# origin: uutils test_sort::test_numeric_unsorted_ints
test test_uu_sort_numeric_unsorted_ints { |ctx|
  fixture_case(ctx, "numeric_unsorted_ints", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_numeric_with_trailing_invalid_chars
test test_uu_sort_numeric_with_trailing_invalid_chars { |ctx|
  fixture_case(ctx, "numeric_trailing_chars", ["-n", "--numeric-sort", "--sort=numeric"])
}

# origin: uutils test_sort::test_output_device
test test_uu_sort_output_device { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-o", "/dev/null"], stdin: bytes.from_text("input"), vars: {})?
  uu.succeeds(r)
  }
}

# origin: uutils test_sort::test_output_file_is_truncated
test test_uu_sort_output_file_is_truncated { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "shrinking", "zzz-leftover-one\nzzz-leftover-two\nzzz-leftover-three\n")
  let r = uu.invoke(s, "sort", ["-o", "shrinking"], stdin: bytes.from_text("kiwi\napple\n"))?
  uu.succeeds(r)
  uu.no_output(r)
  uu.file_is(s, "shrinking", "apple\nkiwi\n")
}

# origin: uutils test_sort::test_output_file_with_leading_dash
test test_uu_sort_output_file_with_leading_dash { |ctx|
  for row in [
    {args: ["--output", "--dash-file"], name: "--dash-file", input: "banana\napple\ncherry\n", expected: "apple\nbanana\ncherry\n"},
    {args: ["-o", "--another-dash-file"], name: "--another-dash-file", input: "zebra\nxray\nyak\n", expected: "xray\nyak\nzebra\n"},
  ] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", row.args, stdin: bytes.from_text(row.input))?
    uu.succeeds(r)
    uu.no_stdout(r)
    uu.file_is(s, row.name, row.expected)
  }
}

# origin: uutils test_sort::test_output_is_input
test test_uu_sort_output_is_input { |ctx|
  let s = uu.scene(ctx)?
  uu.append(s, "file", "a\nb\nc\n")
  let r = uu.invoke(s, "sort", ["-m", "-u", "-o", "file", "file", "file", "file"])?
  uu.succeeds(r)
  uu.file_is(s, "file", "a\nb\nc\n")
}

# origin: uutils test_sort::test_parallel_invalid
test test_uu_sort_parallel_invalid { |ctx|
  for value in ["0", "NaN"] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "sort", [f"--parallel={value}"])?
    uu.fails_with_code(r, 2)
  }
}

# origin: uutils test_sort::test_pipe
test test_uu_sort_pipe { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", [], stdin: bytes.from_text("one\ntwo\nfour"), vars: {})?
  uu.succeeds(r)
  uu.stdout_only(r, "four\none\ntwo\n")
  }
}

# origin: uutils test_sort::test_random_ignore_case
test test_uu_sort_random_ignore_case { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sort", ["-fR"], stdin: bytes.from_text("ABC\nABc\nAbC\nAbc\naBC\naBc\nabC\nabc\n"), vars: {})?
  uu.succeeds(r)
  uu.stdout_is(r, "ABC\nABc\nAbC\nAbc\naBC\naBc\nabC\nabc\n")
  }
}

# origin: uutils test_sort::test_random_shuffle_contains_all_lines
test test_uu_sort_random_shuffle_contains_all_lines { |ctx|
  for option in ["-R"] {
    let s = uu.scene(ctx)?
    let file = "default_unsorted_ints.expected"
    uu.fixture(s, "sort", file, file)
    let expected = uu.read(s, file)?
    let r = uu.invoke(s, "sort", [option, file])?
    uu.succeeds(r)
    let result = r.stdout
    assert result != expected
    let sorted = uu.invoke(s, "sort", [], stdin: result)?
    uu.succeeds(sorted)
    assert sorted.stdout == expected
  }
}

# origin: uutils test_sort::test_random_shuffle_len
test test_uu_sort_random_shuffle_len { |ctx|
  for option in ["-R"] {
    let s = uu.scene(ctx)?
    let file = "default_unsorted_ints.expected"
    uu.fixture(s, "sort", file, file)
    let expected = uu.read(s, file)?
    let r = uu.invoke(s, "sort", [option, file])?
    uu.succeeds(r)
    let result = r.stdout
    assert result != expected
    assert result.len() == expected.len()
  }
}

# origin: uutils test_sort::test_random_shuffle_two_runs_not_the_same
test test_uu_sort_random_shuffle_two_runs_not_the_same { |ctx|
  for option in ["-R", "-k1,1R"] {
    let s = uu.scene(ctx)?
    let file = "default_unsorted_ints.expected"
    uu.fixture(s, "sort", file, file)
    let expected = uu.read(s, file)?
    let r = uu.invoke(s, "sort", [option, file])?
    uu.succeeds(r)
    let result = r.stdout
    assert result != expected
    let second = uu.invoke(s, "sort", [option, file])?
    uu.succeeds(second)
    assert result != second.stdout
  }
}

# origin: uutils test_sort::test_random_source_is_not_read_without_random_sort
test test_uu_sort_random_source_is_not_read_without_random_sort { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "source", b"")
  uu.write(s, "input", "b\na\nc\n")
  let r = uu.invoke(s, "sort", ["--random-source=source", "input"])?
  uu.succeeds(r)
  uu.stdout_is(r, "a\nb\nc\n")
}


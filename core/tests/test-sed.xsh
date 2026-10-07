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
  let unsupported = run.capture --text ${ctx.xsh_bin} $app -- "s/a/b/e" $file
  assert !unsupported.status.exited_with(0)
  assert "unsupported substitution flag" in unsupported.stderr
  let malformed = run.capture --text ${ctx.xsh_bin} $app -- "{p" $file
  assert !malformed.status.exited_with(0)
  assert "unclosed command group" in malformed.stderr
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
  assert "cannot open" in result.stderr
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

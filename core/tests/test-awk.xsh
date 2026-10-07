test test_awk_fields_begin_end_and_precedence { |ctx|
  let input = test.temp_file(ctx, name: "table", contents: b"one 2\ntwo 3\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { print "start" } { total += $2; print NR, $1, 2+3*4 } END { print total }""" $input
  assert output == "start\n1 one 14\n2 two 14\n5\n"
}

test test_awk_arrays_functions_and_loops { |ctx|
  let input = test.temp_file(ctx, name: "keys", contents: b"a\nb\na\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""function double(x) { return x*2 } { count[$1]++ } END { for (key in count) total += count[key]; for (i=0; i<3; i++) sum+=i; print double(total), sum, count["a"] }""" $input
  assert output == "6 3 2\n"
}

test test_awk_regex_substitution_and_printf { |ctx|
  let input = test.temp_file(ctx, name: "text", contents: b"apple 4\nbanana 2\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""/^a/ { gsub(/p/, "P", $1); printf "%s:%04d:%.2f\n", $1, $2, $2/3 }""" $input
  assert output == "aPPle:0004:1.33\n"
}

test test_awk_record_and_field_mutation { |ctx|
  let input = test.temp_file(ctx, name: "csv", contents: b"a,b,c\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- -F , -v OFS=: r"""{ $2="B"; NF=2; print $0, NF; $0="x,y"; print $2, NF }""" $input
  assert output == "a:B:2\ny:2\n"
}

test test_awk_range_next_and_file_counters { |ctx|
  let first = test.temp_file(ctx, name: "first", contents: b"skip\nstart\nend\n")?
  let second = test.temp_file(ctx, name: "second", contents: b"start\nend\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""/start/,/end/ { print NR, FNR } END { print NR }""" $first $second
  assert output == "2 2\n3 3\n4 1\n5 2\n5\n"
}

test test_awk_numeric_strings_and_short_circuit { |ctx|
  let input = test.temp_file(ctx, name: "values", contents: b"0\n10\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""{ if ($1) print ($1 > 2); if (0 && ++n) print "bad" } END { print n+0, "10" < "2" }""" $input
  assert output == "1\n0 1\n"
}

test test_awk_rejects_unsupported_getline { |ctx|
  let result = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""BEGIN { getline x }"""], env: {XSH_MODULE_PATH: ctx.core_dir.display()})?
  assert result.status != 0
  assert "unsupported" in result.stderr
}

test test_awk_array_function_arguments_and_field_increment { |ctx|
  let input = test.temp_file(ctx, name: "numbers", contents: b"2 7\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""function fill(a) { a[1]=9 } { fill(values); print $1++, $1, values[1] }""" $input
  assert output == "2 3 9\n"
}

test test_awk_rejects_redirection { |ctx|
  let result = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""BEGIN { print "hello" > "output" }"""], env: {XSH_MODULE_PATH: ctx.core_dir.display()})?
  assert result.status != 0
  assert "unsupported" in result.stderr
}

test test_awk_operand_assignments_run_between_files { |ctx|
  let first = test.temp_file(ctx, name: "first-assignment", contents: b"a\n")?
  let second = test.temp_file(ctx, name: "second-assignment", contents: b"b\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- -v x=before r"""BEGIN { print x } { print x, $0 } END { print x }""" x=one $first x=two $second
  assert output == "before\none a\ntwo b\ntwo\n"
}

test test_awk_assignment_evaluates_array_index_once { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { i=1; a[i++]++; a[i++]+=4; print i, a[1], a[2] }"""
  assert output == "3 1 4\n"
}

test test_awk_numeric_conversion_formats { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { CONVFMT="%.2f"; OFMT="%.3f"; n=1/3; print n, n ""; printf "%s\n", n }"""
  assert output == "0.333 0.33\n0.33\n"
}

test test_awk_function_next_and_exit_propagate { |ctx|
  let input = test.temp_file(ctx, name: "control", contents: b"skip\nkeep\nstop\nafter\n")?
  let output = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""function control() { if ($1=="skip") next; if ($1=="stop") exit 7 } { control(); print $1 } END { print "end", NR }""", input.display()], env: {XSH_MODULE_PATH: ctx.core_dir.display()})?
  assert output.status == 7
  assert output.stdout == "keep\nend 3\n"
}

test test_awk_assignment_only_operands_read_stdin { |ctx|
  let input = test.temp_file(ctx, name: "assignment-stdin", contents: b"record\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""{ print x, $0 }""" x=value < $input
  assert output == "value record\n"
}

test test_awk_paragraph_records_keep_newline_field_separation { |ctx|
  let input = test.temp_file(ctx, name: "paragraphs", contents: b"\n\na,b\nc,d\n\ne,f\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { RS=""; FS="," } { print NR, NF, $2, $3 }""" $input
  assert output == "1 4 b c\n2 2 f \n"
}

test test_awk_numeric_prefix_conversion_uses_decimal_syntax { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { print "0x10"+0, "inf"+0, "nan"+0, "1e"+0, "  +2.5x"+0 }"""
  assert output == "0 0 0 1 2.5\n"
}

test test_awk_math_builtins_include_atan2 { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { printf "%.5f %.0f %.0f\n", atan2(0,-1), sin(atan2(1,0)), sqrt(9) }"""
  assert output == "3.14159 1 3\n"
}

test test_awk_printf_alternate_general_format_counts_significant_digits { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { printf "%#.6g %.0f %#.0f\n", 0.0012, 2, 2 }"""
  assert output == "0.00120000 2 2.\n"
}

test test_awk_gawk_regex_record_separators_and_rt { |ctx|
  let input = test.temp_file(ctx, name: "records", contents: b"::alpha::beta--omega::")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { RS = "::|--" } { print NR, "[" $0 "]", "[" RT "]" }""" $input
  assert output == "1 [] [::]\n2 [alpha] [::]\n3 [beta] [--]\n4 [omega] [::]\n5 [] []\n"

  let dotted = test.temp_file(ctx, name: "dotted", contents: b"left.right.")?
  let single = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { RS = "." } { print $0, RT }""" $dotted
  assert single == "left .\nright .\n"

  let unseparated = test.temp_file(ctx, name: "unseparated", contents: b"whole")?
  let empty_match = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { RS = "()" } { print NR, $0, "[" RT "]" }""" $unseparated
  assert empty_match == "1 whole []\n"
}

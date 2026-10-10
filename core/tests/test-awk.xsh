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

# POSIX argument processing consults ARGV between files, so edits can change later selections.
test test_awk_argv_file_selection_matrix { |ctx|
  let first = test.temp_file(ctx, name: "argv-first", contents: b"A1\nA2\n")?
  let second = test.temp_file(ctx, name: "argv-second", contents: b"B1\nB2\n")?
  let third = test.temp_file(ctx, name: "argv-third", contents: b"C1\n")?
  let first_name = first.display()
  let second_name = second.display()
  let third_name = third.display()
  let source = fp"{ctx.core_dir}/awk.xsh".read_text()?
  let cases = [
    {
      name: "empty first entry skips that file",
      program: r"""BEGIN { ARGV[1] = "" } { print FILENAME, NR, FNR, $0 } END { print "END", NR, FNR, FILENAME }""",
      files: [first_name, second_name],
      stdout: f"{second_name} 1 1 B1\n{second_name} 2 2 B2\nEND 2 2 {second_name}\n",
    },
    {
      name: "empty middle entry preserves global and per-file counters",
      program: r"""BEGIN { ARGV[2] = "" } { print $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "A1 1 1\nA2 2 2\nC1 3 1\n",
    },
    {
      name: "lower ARGC stops after the first file",
      program: r"""BEGIN { ARGC = 2 } { print $0, NR, FNR } END { print "END", NR }""",
      files: [first_name, second_name, third_name],
      stdout: "A1 1 1\nA2 2 2\nEND 2\n",
    },
    {
      name: "ARGC one selects no input files",
      program: r"""BEGIN { ARGC = 1 } { print "unexpected" } END { print NR, FNR, "[" FILENAME "]" }""",
      files: [first_name, second_name],
      stdout: "0 0 []\n",
    },
    {
      name: "existing file entries can be reordered",
      program: r"""BEGIN { saved = ARGV[1]; ARGV[1] = ARGV[3]; ARGV[3] = saved } { print $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "C1 1 1\nB1 2 1\nB2 3 2\nA1 4 1\nA2 5 2\n",
    },
    {
      name: "an existing input can be selected more than once",
      program: r"""BEGIN { ARGV[2] = ARGV[1] } { print $0, NR, FNR, FILENAME }""",
      files: [first_name, second_name],
      stdout: f"A1 1 1 {first_name}\nA2 2 2 {first_name}\nA1 3 1 {first_name}\nA2 4 2 {first_name}\n",
    },
    {
      name: "an ARGV assignment operand runs before its next file",
      program: r"""{ print "[" tag "]", FILENAME, FNR, $0 }""",
      files: ["tag=early", second_name],
      stdout: f"[early] {second_name} 1 B1\n[early] {second_name} 2 B2\n",
    },
    {
      name: "an assignment operand between files changes later records",
      program: r"""BEGIN { ARGV[2] = "tag=middle" } { print "[" tag "]", $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "[] A1 1 1\n[] A2 2 2\n[middle] C1 3 1\n",
    },
    {
      name: "a new assignment operand is processed below expanded ARGC",
      program: r"""BEGIN { ARGC = 4; ARGV[3] = "tag=tail" } { print "[" tag "]", $0 } END { print "END", tag, NR }""",
      files: [first_name, second_name],
      stdout: "[] A1\n[] A2\n[] B1\n[] B2\nEND tail 4\n",
    },
    {
      name: "deleting a future entry skips its file",
      program: r"""BEGIN { delete ARGV[2] } { print $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "A1 1 1\nA2 2 2\nC1 3 1\n",
    },
    {
      name: "a record action can remove a future file",
      program: r"""NR == 1 { ARGV[2] = "" } { print $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "A1 1 1\nA2 2 2\nC1 3 1\n",
    },
    {
      name: "a record action can reduce ARGC before the next file",
      program: r"""NR == 1 { ARGC = 2 } { print $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "A1 1 1\nA2 2 2\n",
    },
    {
      name: "a record action can replace a future file entry",
      program: r"""NR == 1 { ARGV[2] = ARGV[3]; ARGV[3] = "" } { print $0, NR, FNR }""",
      files: [first_name, second_name, third_name],
      stdout: "A1 1 1\nA2 2 2\nC1 3 1\n",
    },
  ]

  for scenario in cases {
    let result = test.run_script(ctx, source, args: [item for item in [scenario.program] + scenario.files], env: {XSH_MODULE_PATH: ctx.core_dir.display()})?
    assert result.status == 0, f"{scenario.name}: {result.stderr}"
    assert result.stdout == scenario.stdout, scenario.name
  }
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

test test_awk_hex_and_octal_program_literals { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { print or(0xffffffff,1), or(0x80000000,1), or(01234,1), 011 + 1, 0x1F + 0, 08 + 0, 0.5 + 00.5 }"""
  assert output == "4294967295 2147483649 669 10 31 8 1\n"
}

test test_awk_bit_operations { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { print and(12,10), xor(12,10), or(12,10), lshift(3,4), rshift(48,4), rshift(1,1) }"""
  assert output == "8 6 14 48 3 0\n"
}

test test_awk_func_keyword_and_surplus_arguments { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""func f() { print "F" } function g() { print "G"; return 1 } BEGIN { f(g(), g()); print "done" }"""
  assert output == "G\nG\nF\ndone\n"
}

test test_awk_space_before_paren_is_concatenation { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""function f(a) { return a + 1 } BEGIN { v = 1; a = 2; print v (a), f(3), substr ("abc", 2) }"""
  assert output == "12 4 bc\n"
}

test test_awk_postfix_applies_only_to_variables { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { i = 1; print "str" ++i; print "x" i++; print i }"""
  assert output == "str2\nx2\n3\n"
}

test test_awk_assignment_binds_to_its_target_inside_comparison { |ctx|
  let input = test.temp_file(ctx, name: "assign-compare", contents: b"foo\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""$1==$1="foo" {print $1}""" $input
  assert output == "foo\n"
}

test test_awk_bare_length_and_array_length { |ctx|
  let input = test.temp_file(ctx, name: "length-record", contents: b"qwe\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""{ print length, length(), length 1 } END { A[1]; A["qwe"]; print length(A) }""" $input
  assert output == "3 3 31\n2\n"
}

test test_awk_hex_escapes_in_strings_and_separators { |ctx|
  let input = test.temp_file(ctx, name: "hex-separator", contents: b"a!b\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- -F r"""\x21""" r"""{ print $1; print "\x41\x4a!" }""" $input
  assert output == "a\nAJ!\n"
}

test test_awk_braces_are_literal_outside_intervals { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { s = "a{b"; print (s ~ /a{/), ("aa" ~ /^a{2}$/), gsub("{", "-", s), s }"""
  assert output == "1 1 1 a-b\n"
}

test test_awk_print_redirects_only_to_standard_streams { |ctx|
  let result = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""BEGIN { print "err" > "/dev/stderr"; print "out" > "/dev/stdout" }"""], env: {XSH_MODULE_PATH: ctx.core_dir.display()})?
  assert result.status == 0
  assert result.stdout == "out\n"
  assert result.stderr == "err\n"
}

test test_awk_command_getline_reads_shared_stream { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { "printf 'a b\nc\n'" | getline; print $2; "printf 'a b\nc\n'" | getline x; print x, NR; print ("printf ''" | getline z), z "" }"""
  assert output == "b\nc 2\n0 \n"
}

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

test test_awk_getline_reads_stdin_in_begin { |ctx|
  let output = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""BEGIN { getline x; print "got", x } { print "rest", $0 }"""], stdin: b"a\nb\n", env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert output.stdout == "got a\nrest b\n"
}

test test_awk_array_function_arguments_and_field_increment { |ctx|
  let input = test.temp_file(ctx, name: "numbers", contents: b"2 7\n")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""function fill(a) { a[1]=9 } { fill(values); print $1++, $1, values[1] }""" $input
  assert output == "2 3 9\n"
}

test test_awk_redirection_writes_files_and_commands { |ctx|
  let root = test.temp_dir(ctx, name: "redirect")?
  let target = fp"{root}/output"
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- -v target=$target r"""BEGIN { print "hello" > target; print "again" >> target; close(target); while ((getline line < target) > 0) print "read", line; print "b" | "sort"; print "a" | "sort" }"""
  assert output == "read hello\nread again\na\nb\n"
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
  let output = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""function control() { if ($1=="skip") next; if ($1=="stop") exit 7 } { control(); print $1 } END { print "end", NR }""", input], env: {XSH_MODULE_PATH: ctx.core_dir})?
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
      stdout: "0 0 [-]\n",
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
    let result = test.run_script(ctx, source, args: [item for item in [scenario.program] + scenario.files], env: {XSH_MODULE_PATH: ctx.core_dir})?
    assert result.status == 0, f"{scenario.name}: {result.stderr}"
    assert result.stdout == scenario.stdout, scenario.name
  }
}

test test_awk_gawk_regex_record_separators_and_rt { |ctx|
  let input = test.temp_file(ctx, name: "records", contents: b"::alpha::beta--omega::")?
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { RS = "::|--" } { print NR, "[" $0 "]", "[" RT "]" }""" $input
  assert output == "1 [] [::]\n2 [alpha] [::]\n3 [beta] [--]\n4 [omega] [::]\n"

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
  let result = test.run_script(ctx, fp"{ctx.core_dir}/awk.xsh".read_text()?, args: [r"""BEGIN { print "err" > "/dev/stderr"; print "out" > "/dev/stdout" }"""], env: {XSH_MODULE_PATH: ctx.core_dir})?
  assert result.status == 0
  assert result.stdout == "out\n"
  assert result.stderr == "err\n"
}

test test_awk_command_getline_reads_shared_stream { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/awk.xsh" -- r"""BEGIN { "printf 'a b\nc\n'" | getline; print $2; "printf 'a b\nc\n'" | getline x; print x, NR; print ("printf ''" | getline z), z "" }"""
  assert output == "b\nc 0\n0 \n"
}

# Differential matrix: each case in tests/data/awk/cases.json records what GNU
# awk 5.3.2 (default mode, UTF-8 locale) printed for an argument vector, stdin
# and fixture files. Run only against this applet, so no container is needed.
# `xsh` holds the output this implementation intentionally gives instead, and
# `loose` names a channel deliberately left uncompared (regenerate.py documents
# each deviation). The directory the case ran in is rendered as @DIR@.
type MatrixCase = {
  name: Str, args: List[Str], stdin: Str, stdin_b64: Str, files: List[List[Str]], files_b64: List[List[Str]],
  modes: List[List[Str]], links: List[List[Str]], dirs: List[Str], env: List[List[Str]], stdout: Str, stdout_b64: Str,
  stderr: Str, stderr_b64: Str, status: Int, xsh: List[List[Str]], loose: List[Str],
}
type Observed = {stdout: Bytes, stderr: Bytes, status: Int, root: Str}

proc matrix_cases(ctx: TestContext, family: Str) [fs, error] -> Result[List[MatrixCase]] {
  let decoded = json.decode(fp"{ctx.core_dir}/tests/data/awk/matrix/{family}.json".read_text()?)?
  Ok(decoded.require(List[MatrixCase])?)
}

# The value paired with a key in a list of [key, value] pairs.
pure pair_value(pairs: List[List[Str]], key: Str) -> Str? {
  for pair in pairs { return pair[1] when pair[0] == key }
  null
}
pure expected_bytes(case: MatrixCase, channel: Str) -> Result[Bytes] {
  let override = pair_value(case.xsh, channel)
  return Ok(bytes.from_text(override ?? "")) when override != null
  let packed = if channel == "stdout" { case.stdout_b64 } else { case.stderr_b64 }
  return Ok(packed.base64_decode()?) unless packed.is_empty()
  Ok(bytes.from_text(if channel == "stdout" { case.stdout } else { case.stderr }))
}

proc run_case(ctx: TestContext, case: MatrixCase, index: Int) [fs, process, env, error] -> Result[Observed] {
  let root = test.temp_dir(ctx, name: f"awk-case-{index}")?
  for name in case.dirs { fp"{root}/{name}".mkdir() }
  for pair in case.files { fp"{root}/{pair[0]}".write(pair[1]) }
  for pair in case.files_b64 { fp"{root}/{pair[0]}".write(pair[1].base64_decode()?) }
  for pair in case.links { fp"{root}/{pair[0]}".symlink(to: fp"{pair[1]}") }
  for pair in case.modes { fp"{root}/{pair[0]}".chmod(pair[1].parse_int_decimal()?) }
  let stdin = if case.stdin_b64.is_empty() { bytes.from_text(case.stdin) } else { case.stdin_b64.base64_decode()? }
  let arguments = [argument.replace("@DIR@", with: root.display()) for argument in case.args]
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/awk.xsh".display(), "--"] + arguments
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: ""}, stdin, out, err)
  var overlay: Map[Str] = {}
  for pair in case.env { overlay = overlay.set(pair[0], pair[1]) }
  let ran = env (overlay) { process.run(plan) }
  let finished = ran?
  let status = finished?
  Ok({stdout: out.read_bytes()?, stderr: err.read_bytes()?, status: status.exit_code()?, root: root.display()})
}
# The directory path is the one volatile value in the output.
pure normalized(data: Bytes, root: Str) -> Bytes {
  match data.utf8() {
    Ok(text) => bytes.from_text(text.replace(root, with: "@DIR@"))
    Err(_) => data
  }
}

proc check_matrix_shard(ctx: TestContext, family: Str, shard: Int) [fs, process, env, error] -> Result[Unit] {
  let cases = matrix_cases(ctx, family)?
  let start = shard * 8
  assert start < cases.len(), f"empty AWK matrix shard {family}/{shard}"
  let end = if start + 8 < cases.len() { start + 8 } else { cases.len() }
  var failures = []
  for index in range(start, end) {
    let case = cases[index]
    let seen = run_case(ctx, case, index)?
    let root = seen.root
    var problems = []
    if "stdout" not in case.loose and normalized(seen.stdout, root) != expected_bytes(case, "stdout")? { problems += ["stdout"] }
    if "stderr" not in case.loose and normalized(seen.stderr, root) != expected_bytes(case, "stderr")? { problems += ["stderr"] }
    let wanted_status = (pair_value(case.xsh, "status") ?? f"{case.status}").parse_int_decimal()?
    if seen.status != wanted_status { problems += ["status"] }
    if ! problems.is_empty() { failures += [f"{case.name}: {problems.join(",")}"] }
  }
  assert failures.is_empty(), f"{failures.len()} of {end - start} cases differ from the recorded GNU awk output:\n{failures.join("\n")}"
}

test test_awk_matrix_patterns_and_records_shard_000 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 0) }
test test_awk_matrix_patterns_and_records_shard_001 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 1) }
test test_awk_matrix_patterns_and_records_shard_002 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 2) }
test test_awk_matrix_patterns_and_records_shard_003 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 3) }
test test_awk_matrix_patterns_and_records_shard_004 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 4) }
test test_awk_matrix_patterns_and_records_shard_005 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 5) }
test test_awk_matrix_patterns_and_records_shard_006 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 6) }
test test_awk_matrix_patterns_and_records_shard_007 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 7) }
test test_awk_matrix_patterns_and_records_shard_008 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 8) }
test test_awk_matrix_patterns_and_records_shard_009 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 9) }
test test_awk_matrix_patterns_and_records_shard_010 { |ctx| check_matrix_shard(ctx, "patterns_and_records", 10) }
test test_awk_matrix_fields_and_separators_shard_000 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 0) }
test test_awk_matrix_fields_and_separators_shard_001 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 1) }
test test_awk_matrix_fields_and_separators_shard_002 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 2) }
test test_awk_matrix_fields_and_separators_shard_003 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 3) }
test test_awk_matrix_fields_and_separators_shard_004 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 4) }
test test_awk_matrix_fields_and_separators_shard_005 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 5) }
test test_awk_matrix_fields_and_separators_shard_006 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 6) }
test test_awk_matrix_fields_and_separators_shard_007 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 7) }
test test_awk_matrix_fields_and_separators_shard_008 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 8) }
test test_awk_matrix_fields_and_separators_shard_009 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 9) }
test test_awk_matrix_fields_and_separators_shard_010 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 10) }
test test_awk_matrix_fields_and_separators_shard_011 { |ctx| check_matrix_shard(ctx, "fields_and_separators", 11) }
test test_awk_matrix_getline_shard_000 { |ctx| check_matrix_shard(ctx, "getline", 0) }
test test_awk_matrix_getline_shard_001 { |ctx| check_matrix_shard(ctx, "getline", 1) }
test test_awk_matrix_getline_shard_002 { |ctx| check_matrix_shard(ctx, "getline", 2) }
test test_awk_matrix_getline_shard_003 { |ctx| check_matrix_shard(ctx, "getline", 3) }
test test_awk_matrix_getline_shard_004 { |ctx| check_matrix_shard(ctx, "getline", 4) }
test test_awk_matrix_getline_shard_005 { |ctx| check_matrix_shard(ctx, "getline", 5) }
test test_awk_matrix_printf_shard_000 { |ctx| check_matrix_shard(ctx, "printf", 0) }
test test_awk_matrix_printf_shard_001 { |ctx| check_matrix_shard(ctx, "printf", 1) }
test test_awk_matrix_printf_shard_002 { |ctx| check_matrix_shard(ctx, "printf", 2) }
test test_awk_matrix_printf_shard_003 { |ctx| check_matrix_shard(ctx, "printf", 3) }
test test_awk_matrix_printf_shard_004 { |ctx| check_matrix_shard(ctx, "printf", 4) }
test test_awk_matrix_printf_shard_005 { |ctx| check_matrix_shard(ctx, "printf", 5) }
test test_awk_matrix_printf_shard_006 { |ctx| check_matrix_shard(ctx, "printf", 6) }
test test_awk_matrix_printf_shard_007 { |ctx| check_matrix_shard(ctx, "printf", 7) }
test test_awk_matrix_printf_shard_008 { |ctx| check_matrix_shard(ctx, "printf", 8) }
test test_awk_matrix_printf_shard_009 { |ctx| check_matrix_shard(ctx, "printf", 9) }
test test_awk_matrix_printf_shard_010 { |ctx| check_matrix_shard(ctx, "printf", 10) }
test test_awk_matrix_printf_shard_011 { |ctx| check_matrix_shard(ctx, "printf", 11) }
test test_awk_matrix_printf_shard_012 { |ctx| check_matrix_shard(ctx, "printf", 12) }
test test_awk_matrix_printf_shard_013 { |ctx| check_matrix_shard(ctx, "printf", 13) }
test test_awk_matrix_printf_shard_014 { |ctx| check_matrix_shard(ctx, "printf", 14) }
test test_awk_matrix_printf_shard_015 { |ctx| check_matrix_shard(ctx, "printf", 15) }
test test_awk_matrix_printf_shard_016 { |ctx| check_matrix_shard(ctx, "printf", 16) }
test test_awk_matrix_printf_shard_017 { |ctx| check_matrix_shard(ctx, "printf", 17) }
test test_awk_matrix_printf_shard_018 { |ctx| check_matrix_shard(ctx, "printf", 18) }
test test_awk_matrix_printf_shard_019 { |ctx| check_matrix_shard(ctx, "printf", 19) }
test test_awk_matrix_strings_shard_000 { |ctx| check_matrix_shard(ctx, "strings", 0) }
test test_awk_matrix_strings_shard_001 { |ctx| check_matrix_shard(ctx, "strings", 1) }
test test_awk_matrix_strings_shard_002 { |ctx| check_matrix_shard(ctx, "strings", 2) }
test test_awk_matrix_strings_shard_003 { |ctx| check_matrix_shard(ctx, "strings", 3) }
test test_awk_matrix_strings_shard_004 { |ctx| check_matrix_shard(ctx, "strings", 4) }
test test_awk_matrix_strings_shard_005 { |ctx| check_matrix_shard(ctx, "strings", 5) }
test test_awk_matrix_strings_shard_006 { |ctx| check_matrix_shard(ctx, "strings", 6) }
test test_awk_matrix_strings_shard_007 { |ctx| check_matrix_shard(ctx, "strings", 7) }
test test_awk_matrix_strings_shard_008 { |ctx| check_matrix_shard(ctx, "strings", 8) }
test test_awk_matrix_strings_shard_009 { |ctx| check_matrix_shard(ctx, "strings", 9) }
test test_awk_matrix_strings_shard_010 { |ctx| check_matrix_shard(ctx, "strings", 10) }
test test_awk_matrix_strings_shard_011 { |ctx| check_matrix_shard(ctx, "strings", 11) }
test test_awk_matrix_numbers_shard_000 { |ctx| check_matrix_shard(ctx, "numbers", 0) }
test test_awk_matrix_numbers_shard_001 { |ctx| check_matrix_shard(ctx, "numbers", 1) }
test test_awk_matrix_numbers_shard_002 { |ctx| check_matrix_shard(ctx, "numbers", 2) }
test test_awk_matrix_numbers_shard_003 { |ctx| check_matrix_shard(ctx, "numbers", 3) }
test test_awk_matrix_numbers_shard_004 { |ctx| check_matrix_shard(ctx, "numbers", 4) }
test test_awk_matrix_numbers_shard_005 { |ctx| check_matrix_shard(ctx, "numbers", 5) }
test test_awk_matrix_numbers_shard_006 { |ctx| check_matrix_shard(ctx, "numbers", 6) }
test test_awk_matrix_numbers_shard_007 { |ctx| check_matrix_shard(ctx, "numbers", 7) }
test test_awk_matrix_numbers_shard_008 { |ctx| check_matrix_shard(ctx, "numbers", 8) }
test test_awk_matrix_arrays_and_functions_shard_000 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 0) }
test test_awk_matrix_arrays_and_functions_shard_001 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 1) }
test test_awk_matrix_arrays_and_functions_shard_002 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 2) }
test test_awk_matrix_arrays_and_functions_shard_003 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 3) }
test test_awk_matrix_arrays_and_functions_shard_004 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 4) }
test test_awk_matrix_arrays_and_functions_shard_005 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 5) }
test test_awk_matrix_arrays_and_functions_shard_006 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 6) }
test test_awk_matrix_arrays_and_functions_shard_007 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 7) }
test test_awk_matrix_arrays_and_functions_shard_008 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 8) }
test test_awk_matrix_arrays_and_functions_shard_009 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 9) }
test test_awk_matrix_arrays_and_functions_shard_010 { |ctx| check_matrix_shard(ctx, "arrays_and_functions", 10) }
test test_awk_matrix_streams_and_processes_shard_000 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 0) }
test test_awk_matrix_streams_and_processes_shard_001 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 1) }
test test_awk_matrix_streams_and_processes_shard_002 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 2) }
test test_awk_matrix_streams_and_processes_shard_003 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 3) }
test test_awk_matrix_streams_and_processes_shard_004 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 4) }
test test_awk_matrix_streams_and_processes_shard_005 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 5) }
test test_awk_matrix_streams_and_processes_shard_006 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 6) }
test test_awk_matrix_streams_and_processes_shard_007 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 7) }
test test_awk_matrix_streams_and_processes_shard_008 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 8) }
test test_awk_matrix_streams_and_processes_shard_009 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 9) }
test test_awk_matrix_streams_and_processes_shard_010 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 10) }
test test_awk_matrix_streams_and_processes_shard_011 { |ctx| check_matrix_shard(ctx, "streams_and_processes", 11) }
test test_awk_matrix_options_and_uninitialized_shard_000 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 0) }
test test_awk_matrix_options_and_uninitialized_shard_001 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 1) }
test test_awk_matrix_options_and_uninitialized_shard_002 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 2) }
test test_awk_matrix_options_and_uninitialized_shard_003 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 3) }
test test_awk_matrix_options_and_uninitialized_shard_004 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 4) }
test test_awk_matrix_options_and_uninitialized_shard_005 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 5) }
test test_awk_matrix_options_and_uninitialized_shard_006 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 6) }
test test_awk_matrix_options_and_uninitialized_shard_007 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 7) }
test test_awk_matrix_options_and_uninitialized_shard_008 { |ctx| check_matrix_shard(ctx, "options_and_uninitialized", 8) }
test test_awk_matrix_grammar_shard_000 { |ctx| check_matrix_shard(ctx, "grammar", 0) }
test test_awk_matrix_grammar_shard_001 { |ctx| check_matrix_shard(ctx, "grammar", 1) }
test test_awk_matrix_grammar_shard_002 { |ctx| check_matrix_shard(ctx, "grammar", 2) }
test test_awk_matrix_grammar_shard_003 { |ctx| check_matrix_shard(ctx, "grammar", 3) }
test test_awk_matrix_grammar_shard_004 { |ctx| check_matrix_shard(ctx, "grammar", 4) }
test test_awk_matrix_grammar_shard_005 { |ctx| check_matrix_shard(ctx, "grammar", 5) }
test test_awk_matrix_grammar_shard_006 { |ctx| check_matrix_shard(ctx, "grammar", 6) }
test test_awk_matrix_grammar_shard_007 { |ctx| check_matrix_shard(ctx, "grammar", 7) }
test test_awk_matrix_grammar_shard_008 { |ctx| check_matrix_shard(ctx, "grammar", 8) }
test test_awk_matrix_grammar_shard_009 { |ctx| check_matrix_shard(ctx, "grammar", 9) }
test test_awk_matrix_grammar_shard_010 { |ctx| check_matrix_shard(ctx, "grammar", 10) }
test test_awk_matrix_grammar_shard_011 { |ctx| check_matrix_shard(ctx, "grammar", 11) }
test test_awk_matrix_grammar_shard_012 { |ctx| check_matrix_shard(ctx, "grammar", 12) }
test test_awk_matrix_regex_shard_000 { |ctx| check_matrix_shard(ctx, "regex", 0) }
test test_awk_matrix_regex_shard_001 { |ctx| check_matrix_shard(ctx, "regex", 1) }
test test_awk_matrix_regex_shard_002 { |ctx| check_matrix_shard(ctx, "regex", 2) }
test test_awk_matrix_regex_shard_003 { |ctx| check_matrix_shard(ctx, "regex", 3) }
test test_awk_matrix_regex_shard_004 { |ctx| check_matrix_shard(ctx, "regex", 4) }
test test_awk_matrix_regex_shard_005 { |ctx| check_matrix_shard(ctx, "regex", 5) }
test test_awk_matrix_regex_shard_006 { |ctx| check_matrix_shard(ctx, "regex", 6) }
test test_awk_matrix_regex_shard_007 { |ctx| check_matrix_shard(ctx, "regex", 7) }
test test_awk_matrix_regex_shard_008 { |ctx| check_matrix_shard(ctx, "regex", 8) }
test test_awk_matrix_regex_shard_009 { |ctx| check_matrix_shard(ctx, "regex", 9) }
test test_awk_matrix_regex_shard_010 { |ctx| check_matrix_shard(ctx, "regex", 10) }
test test_awk_matrix_regex_shard_011 { |ctx| check_matrix_shard(ctx, "regex", 11) }
test test_awk_matrix_for_in_order_and_non_utf8_bytes_shard_000 { |ctx| check_matrix_shard(ctx, "for_in_order_and_non_utf8_bytes", 0) }
test test_awk_matrix_for_in_order_and_non_utf8_bytes_shard_001 { |ctx| check_matrix_shard(ctx, "for_in_order_and_non_utf8_bytes", 1) }
test test_awk_matrix_for_in_order_and_non_utf8_bytes_shard_002 { |ctx| check_matrix_shard(ctx, "for_in_order_and_non_utf8_bytes", 2) }
test test_awk_matrix_for_in_order_and_non_utf8_bytes_shard_003 { |ctx| check_matrix_shard(ctx, "for_in_order_and_non_utf8_bytes", 3) }
test test_awk_matrix_for_in_order_and_non_utf8_bytes_shard_004 { |ctx| check_matrix_shard(ctx, "for_in_order_and_non_utf8_bytes", 4) }
test test_awk_matrix_for_in_order_and_non_utf8_bytes_shard_005 { |ctx| check_matrix_shard(ctx, "for_in_order_and_non_utf8_bytes", 5) }
test test_awk_matrix_lexical_forms_shard_000 { |ctx| check_matrix_shard(ctx, "lexical_forms", 0) }
test test_awk_matrix_lexical_forms_shard_001 { |ctx| check_matrix_shard(ctx, "lexical_forms", 1) }
test test_awk_matrix_lexical_forms_shard_002 { |ctx| check_matrix_shard(ctx, "lexical_forms", 2) }
test test_awk_matrix_lexical_forms_shard_003 { |ctx| check_matrix_shard(ctx, "lexical_forms", 3) }
test test_awk_matrix_lexical_forms_shard_004 { |ctx| check_matrix_shard(ctx, "lexical_forms", 4) }
test test_awk_matrix_lexical_forms_shard_005 { |ctx| check_matrix_shard(ctx, "lexical_forms", 5) }
test test_awk_matrix_lexical_forms_shard_006 { |ctx| check_matrix_shard(ctx, "lexical_forms", 6) }
test test_awk_matrix_lexical_forms_shard_007 { |ctx| check_matrix_shard(ctx, "lexical_forms", 7) }
test test_awk_matrix_special_variables_shard_000 { |ctx| check_matrix_shard(ctx, "special_variables", 0) }
test test_awk_matrix_special_variables_shard_001 { |ctx| check_matrix_shard(ctx, "special_variables", 1) }
test test_awk_matrix_special_variables_shard_002 { |ctx| check_matrix_shard(ctx, "special_variables", 2) }
test test_awk_matrix_special_variables_shard_003 { |ctx| check_matrix_shard(ctx, "special_variables", 3) }
test test_awk_matrix_special_variables_shard_004 { |ctx| check_matrix_shard(ctx, "special_variables", 4) }
test test_awk_matrix_special_variables_shard_005 { |ctx| check_matrix_shard(ctx, "special_variables", 5) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_000 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 0) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_001 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 1) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_002 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 2) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_003 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 3) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_004 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 4) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_005 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 5) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_006 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 6) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_007 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 7) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_008 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 8) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_009 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 9) }
test test_awk_matrix_declaration_and_newline_diagnostics_shard_010 { |ctx| check_matrix_shard(ctx, "declaration_and_newline_diagnostics", 10) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_000 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 0) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_001 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 1) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_002 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 2) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_003 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 3) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_004 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 4) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_005 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 5) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_006 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 6) }
test test_awk_matrix_rule_and_regexp_constant_diagnostics_shard_007 { |ctx| check_matrix_shard(ctx, "rule_and_regexp_constant_diagnostics", 7) }
test test_awk_matrix_source_end_diagnostics_shard_000 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 0) }
test test_awk_matrix_source_end_diagnostics_shard_001 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 1) }
test test_awk_matrix_source_end_diagnostics_shard_002 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 2) }
test test_awk_matrix_source_end_diagnostics_shard_003 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 3) }
test test_awk_matrix_source_end_diagnostics_shard_004 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 4) }
test test_awk_matrix_source_end_diagnostics_shard_005 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 5) }
test test_awk_matrix_source_end_diagnostics_shard_006 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 6) }
test test_awk_matrix_source_end_diagnostics_shard_007 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 7) }
test test_awk_matrix_source_end_diagnostics_shard_008 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 8) }
test test_awk_matrix_source_end_diagnostics_shard_009 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 9) }
test test_awk_matrix_source_end_diagnostics_shard_010 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 10) }
test test_awk_matrix_source_end_diagnostics_shard_011 { |ctx| check_matrix_shard(ctx, "source_end_diagnostics", 11) }
test test_awk_matrix_untyped_variables_shard_000 { |ctx| check_matrix_shard(ctx, "untyped_variables", 0) }
test test_awk_matrix_untyped_variables_shard_001 { |ctx| check_matrix_shard(ctx, "untyped_variables", 1) }
test test_awk_matrix_untyped_variables_shard_002 { |ctx| check_matrix_shard(ctx, "untyped_variables", 2) }
test test_awk_matrix_untyped_variables_shard_003 { |ctx| check_matrix_shard(ctx, "untyped_variables", 3) }
test test_awk_matrix_untyped_variables_shard_004 { |ctx| check_matrix_shard(ctx, "untyped_variables", 4) }
test test_awk_matrix_untyped_variables_shard_005 { |ctx| check_matrix_shard(ctx, "untyped_variables", 5) }
test test_awk_matrix_untyped_variables_shard_006 { |ctx| check_matrix_shard(ctx, "untyped_variables", 6) }
test test_awk_matrix_untyped_variables_shard_007 { |ctx| check_matrix_shard(ctx, "untyped_variables", 7) }
test test_awk_matrix_untyped_variables_shard_008 { |ctx| check_matrix_shard(ctx, "untyped_variables", 8) }
test test_awk_matrix_untyped_variables_shard_009 { |ctx| check_matrix_shard(ctx, "untyped_variables", 9) }
test test_awk_matrix_untyped_variables_shard_010 { |ctx| check_matrix_shard(ctx, "untyped_variables", 10) }
test test_awk_matrix_untyped_variables_shard_011 { |ctx| check_matrix_shard(ctx, "untyped_variables", 11) }
test test_awk_matrix_assignment_targets_shard_000 { |ctx| check_matrix_shard(ctx, "assignment_targets", 0) }
test test_awk_matrix_assignment_targets_shard_001 { |ctx| check_matrix_shard(ctx, "assignment_targets", 1) }
test test_awk_matrix_assignment_targets_shard_002 { |ctx| check_matrix_shard(ctx, "assignment_targets", 2) }
test test_awk_matrix_assignment_targets_shard_003 { |ctx| check_matrix_shard(ctx, "assignment_targets", 3) }
test test_awk_matrix_assignment_targets_shard_004 { |ctx| check_matrix_shard(ctx, "assignment_targets", 4) }
test test_awk_matrix_assignment_targets_shard_005 { |ctx| check_matrix_shard(ctx, "assignment_targets", 5) }
test test_awk_matrix_assignment_targets_shard_006 { |ctx| check_matrix_shard(ctx, "assignment_targets", 6) }
test test_awk_matrix_assignment_targets_shard_007 { |ctx| check_matrix_shard(ctx, "assignment_targets", 7) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_000 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 0) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_001 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 1) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_002 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 2) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_003 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 3) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_004 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 4) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_005 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 5) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_006 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 6) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_007 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 7) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_008 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 8) }
test test_awk_matrix_arithmetic_and_bit_functions_shard_009 { |ctx| check_matrix_shard(ctx, "arithmetic_and_bit_functions", 9) }
test test_awk_matrix_builtin_argument_counts_shard_000 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 0) }
test test_awk_matrix_builtin_argument_counts_shard_001 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 1) }
test test_awk_matrix_builtin_argument_counts_shard_002 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 2) }
test test_awk_matrix_builtin_argument_counts_shard_003 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 3) }
test test_awk_matrix_builtin_argument_counts_shard_004 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 4) }
test test_awk_matrix_builtin_argument_counts_shard_005 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 5) }
test test_awk_matrix_builtin_argument_counts_shard_006 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 6) }
test test_awk_matrix_builtin_argument_counts_shard_007 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 7) }
test test_awk_matrix_builtin_argument_counts_shard_008 { |ctx| check_matrix_shard(ctx, "builtin_argument_counts", 8) }
test test_awk_matrix_regexp_syntax_first_half_shard_000 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 0) }
test test_awk_matrix_regexp_syntax_first_half_shard_001 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 1) }
test test_awk_matrix_regexp_syntax_first_half_shard_002 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 2) }
test test_awk_matrix_regexp_syntax_first_half_shard_003 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 3) }
test test_awk_matrix_regexp_syntax_first_half_shard_004 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 4) }
test test_awk_matrix_regexp_syntax_first_half_shard_005 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 5) }
test test_awk_matrix_regexp_syntax_first_half_shard_006 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 6) }
test test_awk_matrix_regexp_syntax_first_half_shard_007 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 7) }
test test_awk_matrix_regexp_syntax_first_half_shard_008 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 8) }
test test_awk_matrix_regexp_syntax_first_half_shard_009 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 9) }
test test_awk_matrix_regexp_syntax_first_half_shard_010 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 10) }
test test_awk_matrix_regexp_syntax_first_half_shard_011 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 11) }
test test_awk_matrix_regexp_syntax_first_half_shard_012 { |ctx| check_matrix_shard(ctx, "regexp_syntax_first_half", 12) }
test test_awk_matrix_regexp_syntax_second_half_shard_000 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 0) }
test test_awk_matrix_regexp_syntax_second_half_shard_001 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 1) }
test test_awk_matrix_regexp_syntax_second_half_shard_002 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 2) }
test test_awk_matrix_regexp_syntax_second_half_shard_003 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 3) }
test test_awk_matrix_regexp_syntax_second_half_shard_004 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 4) }
test test_awk_matrix_regexp_syntax_second_half_shard_005 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 5) }
test test_awk_matrix_regexp_syntax_second_half_shard_006 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 6) }
test test_awk_matrix_regexp_syntax_second_half_shard_007 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 7) }
test test_awk_matrix_regexp_syntax_second_half_shard_008 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 8) }
test test_awk_matrix_regexp_syntax_second_half_shard_009 { |ctx| check_matrix_shard(ctx, "regexp_syntax_second_half", 9) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_000 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 0) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_001 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 1) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_002 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 2) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_003 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 3) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_004 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 4) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_005 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 5) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_006 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 6) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_007 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 7) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_008 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 8) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_009 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 9) }
test test_awk_matrix_gnu_variables_and_field_modes_shard_010 { |ctx| check_matrix_shard(ctx, "gnu_variables_and_field_modes", 10) }
test test_awk_matrix_program_files_and_includes_shard_000 { |ctx| check_matrix_shard(ctx, "program_files_and_includes", 0) }
test test_awk_matrix_program_files_and_includes_shard_001 { |ctx| check_matrix_shard(ctx, "program_files_and_includes", 1) }
test test_awk_matrix_program_files_and_includes_shard_002 { |ctx| check_matrix_shard(ctx, "program_files_and_includes", 2) }
test test_awk_matrix_program_files_and_includes_shard_003 { |ctx| check_matrix_shard(ctx, "program_files_and_includes", 3) }
test test_awk_matrix_dialect_options_shard_000 { |ctx| check_matrix_shard(ctx, "dialect_options", 0) }
test test_awk_matrix_dialect_options_shard_001 { |ctx| check_matrix_shard(ctx, "dialect_options", 1) }
test test_awk_matrix_dialect_options_shard_002 { |ctx| check_matrix_shard(ctx, "dialect_options", 2) }
test test_awk_matrix_dialect_options_shard_003 { |ctx| check_matrix_shard(ctx, "dialect_options", 3) }
test test_awk_matrix_dialect_options_shard_004 { |ctx| check_matrix_shard(ctx, "dialect_options", 4) }
test test_awk_matrix_dialect_options_shard_005 { |ctx| check_matrix_shard(ctx, "dialect_options", 5) }
test test_awk_matrix_dialect_options_shard_006 { |ctx| check_matrix_shard(ctx, "dialect_options", 6) }
test test_awk_matrix_dialect_options_shard_007 { |ctx| check_matrix_shard(ctx, "dialect_options", 7) }
test test_awk_matrix_dialect_options_shard_008 { |ctx| check_matrix_shard(ctx, "dialect_options", 8) }
test test_awk_matrix_dialect_options_shard_009 { |ctx| check_matrix_shard(ctx, "dialect_options", 9) }
test test_awk_matrix_dialect_options_shard_010 { |ctx| check_matrix_shard(ctx, "dialect_options", 10) }
test test_awk_matrix_dialect_options_shard_011 { |ctx| check_matrix_shard(ctx, "dialect_options", 11) }
test test_awk_matrix_dialect_options_shard_012 { |ctx| check_matrix_shard(ctx, "dialect_options", 12) }
test test_awk_matrix_dialect_options_shard_013 { |ctx| check_matrix_shard(ctx, "dialect_options", 13) }
test test_awk_matrix_dialect_options_shard_014 { |ctx| check_matrix_shard(ctx, "dialect_options", 14) }
test test_awk_matrix_dialect_options_shard_015 { |ctx| check_matrix_shard(ctx, "dialect_options", 15) }
test test_awk_matrix_dialect_options_shard_016 { |ctx| check_matrix_shard(ctx, "dialect_options", 16) }
test test_awk_matrix_dialect_options_shard_017 { |ctx| check_matrix_shard(ctx, "dialect_options", 17) }
test test_awk_matrix_dialect_options_shard_018 { |ctx| check_matrix_shard(ctx, "dialect_options", 18) }
test test_awk_matrix_dialect_options_shard_019 { |ctx| check_matrix_shard(ctx, "dialect_options", 19) }
test test_awk_matrix_dialect_options_shard_020 { |ctx| check_matrix_shard(ctx, "dialect_options", 20) }

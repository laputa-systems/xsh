# Run jq.xsh with a program and JSON input on stdin, returning compact stdout.
proc run_jq(ctx: TestContext, program: Str, input: Str) [fs, process, error] -> Result[Str] {
  let infile = test.temp_file(ctx, name: "in.json", contents: bytes.from_text(input))?

  return run.text (
    "xsh"
    "showcase/jq.xsh"
    "--"
    "-c"
    $program
    < $infile
  ) ?
}

test test_jq_identity [fs, process, error] { |ctx|
  let out = run_jq(ctx, ".", "{\"b\":2,\"a\":1}")?

  # Object key insertion order is preserved (not sorted).
  test.eq(out.trim(), "{\"b\":2,\"a\":1}")?
}

test test_jq_number_roundtrip [fs, process, error] { |ctx|
  let out = run_jq(ctx, ".", "[1, 2.5, -100, 1000]")?
  test.eq(out.trim(), "[1,2.5,-100,1000]")?
}

test test_jq_stream [fs, process, error] { |ctx|
  let out = run_jq(ctx, ".", "1 \"x\" null true")?

  test.eq(
    out.trim(),
    """1
"x"
null
true""",
  )?
}

test test_jq_rejects_misspelled_json_literals [fs, process, error] { |ctx|
  for invalid in ["noll", "trux", "falsx"] {
    let input = test.temp_file(ctx, name: f"${invalid}.json", contents: bytes.from_text(invalid))?
    let diagnostic = test.temp_path(ctx, name: f"${invalid}.err")
    let status = run.status xsh showcase/jq.xsh -- -c . < $input 2> $diagnostic
    test.ok(! status.exited_with(0))?
    test.contains(diagnostic.read_text()?, "Invalid JSON value")?
  }
}

test test_jq_pipe_index [fs, process, error] { |ctx|
  let out = run_jq(ctx, ".a | .[1]", "{\"a\":[10,20,30]}")?
  test.eq(out.trim(), "20")?
}

test test_jq_construct [fs, process, error] { |ctx|
  let out = run_jq(ctx, "{x: .a, y: (.b + 1)}", "{\"a\":1,\"b\":2}")?
  test.eq(out.trim(), "{\"x\":1,\"y\":3}")?
}

test test_jq_arith_stream [fs, process, error] { |ctx|
  let out = run_jq(ctx, "(1,2)+(10,20)", "null")?

  test.eq(
    out.trim(),
    """11
12
21
22""",
  )?
}

test test_jq_assign [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, ".a = 5", "{\"a\":1,\"b\":2}")?.trim(), "{\"a\":5,\"b\":2}")?
  test.eq(run_jq(ctx, ".a |= .+1", "{\"a\":1}")?.trim(), "{\"a\":2}")?
  test.eq(run_jq(ctx, ".a.b += 1", "{\"a\":{\"b\":2}}")?.trim(), "{\"a\":{\"b\":3}}")?
  test.eq(run_jq(ctx, ".[] |= .+1", "[1,2,3]")?.trim(), "[2,3,4]")?
}

test test_jq_paths [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, "del(.a)", "{\"a\":1,\"b\":2}")?.trim(), "{\"b\":2}")?
  test.eq(run_jq(ctx, "del(.[1])", "[1,2,3]")?.trim(), "[1,3]")?
  test.eq(run_jq(ctx, "getpath([\"a\",\"b\"])", "{\"a\":{\"b\":5}}")?.trim(), "5")?
  test.eq(run_jq(ctx, "[paths]", "{\"a\":[1]}")?.trim(), "[[\"a\"],[\"a\",0]]")?
}

test test_jq_builtins [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, "[.[]|.+1]", "[1,2,3]")?.trim(), "[2,3,4]")?
  test.eq(run_jq(ctx, "map(select(.>2))", "[1,2,3,4]")?.trim(), "[3,4]")?
  test.eq(run_jq(ctx, "sort_by(.x)", "[{\"x\":3},{\"x\":1}]")?.trim(), "[{\"x\":1},{\"x\":3}]")?
  test.eq(run_jq(ctx, "group_by(.%2)", "[1,2,3,4]")?.trim(), "[[2,4],[1,3]]")?
  test.eq(run_jq(ctx, "to_entries", "{\"a\":1}")?.trim(), "[{\"key\":\"a\",\"value\":1}]")?
  test.eq(run_jq(ctx, "add", "[1,2,3]")?.trim(), "6")?
}

test test_jq_strings [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, "\"\\(.x) and \\(.y)\"", "{\"x\":1,\"y\":2}")?.trim(), "\"1 and 2\"")?
  test.eq(run_jq(ctx, "@base64", "\"hello\"")?.trim(), "\"aGVsbG8=\"")?
  test.eq(run_jq(ctx, "ascii_downcase", "\"ABc\"")?.trim(), "\"abc\"")?
  test.eq(run_jq(ctx, "split(\",\")", "\"a,b,c\"")?.trim(), "[\"a\",\"b\",\"c\"]")?
  test.eq(run_jq(ctx, "[1,\"x,y\"]|@csv", "null")?.trim(), "\"1,\\\"x,y\\\"\"")?
}

test test_jq_bindings [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, "5 as $x | $x + 1", "null")?.trim(), "6")?
  test.eq(run_jq(ctx, ". as [$a,$b] | $a+$b", "[3,4]")?.trim(), "7")?
  test.eq(run_jq(ctx, "reduce .[] as $x (0; .+$x)", "[1,2,3,4]")?.trim(), "10")?
  test.eq(run_jq(ctx, "[foreach .[] as $x (0; .+$x)]", "[1,2,3]")?.trim(), "[1,3,6]")?
}

test test_jq_defs [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, "def inc: .+1; inc", "5")?.trim(), "6")?
  test.eq(run_jq(ctx, "def f(g): g+g; f(.+1)", "10")?.trim(), "22")?
  let fact = "def fact: if . <= 1 then 1 else . * (. - 1 | fact) end; fact"
  test.eq(run_jq(ctx, fact, "4")?.trim(), "24")?
  test.eq(run_jq(ctx, "[limit(2; .[])]", "[1,2,3,4]")?.trim(), "[1,2]")?
}

test test_jq_regex [fs, process, error] { |ctx|
  test.eq(run_jq(ctx, "test(\"a\")", "\"cat\"")?.trim(), "true")?
  test.eq(run_jq(ctx, "test(\"A\";\"i\")", "\"cat\"")?.trim(), "true")?
  test.eq(run_jq(ctx, "[scan(\"[a-z]+\")]", "\"a1bc2def\"")?.trim(), "[\"a\",\"bc\",\"def\"]")?
  test.eq(run_jq(ctx, "gsub(\"a\";\"X\")", "\"banana\"")?.trim(), "\"bXnXnX\"")?
  test.eq(run_jq(ctx, "sub(\"a\";\"X\")", "\"banana\"")?.trim(), "\"bXnana\"")?
}

test test_jq_alt_and_try [fs, process, error] { |ctx|
  let out = run_jq(ctx, ".a // \"def\"", "{\"b\":1}")?
  test.eq(out.trim(), "\"def\"")?
  let out2 = run_jq(ctx, "try error(\"boom\") catch .", "null")?
  test.eq(out2.trim(), "\"boom\"")?
}

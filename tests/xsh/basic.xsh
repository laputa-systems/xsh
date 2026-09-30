test test_pass [error] {
  1 == 1
}

test test_guard_failure_controls_enclosing_loop [error] {
  var numbers: List[Int] = []
  for source in ["1", "invalid", "2"] {
    guard let number = source.parse_int() else { |_|
      continue
    }

    numbers = numbers.push(number)
  }

  numbers == [1, 2]

  numbers = []
  for source in ["1", "invalid", "2"] {
    guard let number = source.parse_int() else { |_|
      break
    }

    numbers = numbers.push(number)
  }

  numbers == [1]
}

test test_typed_integer_augmented_assignment_keeps_results_and_errors [error] { |ctx|
  var value: Int = 13
  value += 5
  value -= 2
  value *= 3
  value /= 6
  value %= 3
  value == 2

  let overflow = test.run_script(
    ctx,
    """var value: Int = 9223372036854775807
value += 1
""",
  )?
  test.ok(! overflow.success, overflow.stderr)?
  "integer-overflow" in overflow.stderr

  let division = test.run_script(
    ctx,
    """var value: Int = 8
value /= 0
""",
  )?
  test.ok(! division.success, division.stderr)?
  "division-by-zero" in division.stderr
}

pure sibling_branch_value(choice: Str) -> Int {
  if choice == "first" {
    let value = 0
    return value
  } else if choice == "second" {
    let value = 10
    return value
  } else {
    let value = 20
    return value
  }
}

test test_sibling_if_branches_keep_their_own_local_bindings [error] {
  sibling_branch_value("first") == 0
  sibling_branch_value("second") == 10
  sibling_branch_value("other") == 20
  let choice = "second"
  if choice == "first" {
    let value = 0
    value == 0
  } else if choice == "second" {
    let value = 10
    value == 10
  }
}

test test_repeated_if_branches_select_statement_and_expression_arms [error] {
  var total = 0
  for value in [0, 1, 2, 3, 4, 5] {
    if value % 3 == 0 {
      total += 1
    } else if value % 3 == 1 {
      total += 10
    } else {
      total += 100
    }

    let label = if value % 3 == 0 { "first" } else if value % 3 == 1 { "second" } else { "third" }
    label == ["first", "second", "third"][value % 3]
  }

  total == 222
}

pure locally_selected_arguments(argv: List[Str]) -> List[Str] {
  return if argv.len() > 0 and argv[0] == "--" { [] } else { argv }
}

test test_local_args_shadows_predeclared_script_arguments [error] {
  locally_selected_arguments(["unknown"]) == ["unknown"]
  locally_selected_arguments([]) == []
  let argv = ["unknown"]
  let selected = if argv.len() > 0 and argv[0] == "--" { [] } else { argv }
  selected == ["unknown"]
}

test test_skip {
  test.skip("later")
}

test test_temp [fs, error] { |ctx|
  let one = test.temp_path(ctx)
  let two = test.temp_path(ctx)
  one != two
  let file = test.temp_file(ctx, name: "data", contents: b"ok")?
  let data = fs.read_text(file)?
  data == "ok"
}

test test_process_command_builder [process, error] {
  let command = process.command {
    run true
  }

  let status = process.run(command)?
  test.ok(status.exited_with(0), "builder command should run")?
}

pure language_sugar_label(value: Str) -> Result[Str] {
  value
}

pure language_sugar_returned(value: Str) -> Result[Str] {
  return value
}

test test_language_sugar_edge_cases [fs, error] { |ctx|
  let root = test.temp_dir(ctx, name: "language-sugar")?
  let file = fp"${root}/note.txt"
  file.write("""alpha
beta
""")?
  let content = file.read_text()?
  let raw = r"\n ${literal}"
  let nested = f"""${{name: "demo"}.name}:${if true { "x}" } else { "y" }}:${f"${1}"}"""
  let escaped = f"\${not_interp}:${"ok"}:ca"
  let names = fs.children(root) |> map .name

  language_sugar_label("ok")? == "ok"
  language_sugar_returned("return")? == "return"
  content == """alpha
beta
"""
  raw == r"\n ${literal}"
  nested == "demo:x}:1"
  escaped == "\${not_interp}:ok:ca"
  names[0] == "note.txt"
}

test test_dns_mock [net, error] { |ctx|
  test.mock(
    ctx,
    "dns.lookup",
    {name: "example.test"},
    Ok([{name: "example.test", record: "A", value: "127.0.0.1", ttl: 60}]),
  )?

  let records = dns.lookup("example.test")?
  records[0].value == "127.0.0.1"
  let calls = test.calls(ctx, "dns.lookup")
  calls.len() == 1
}

test test_net_mock [net, error] { |ctx|
  test.mock(
    ctx,
    "net.request",
    {url: "https://example.test/"},
    Ok({
      status: 200,
      reason: "OK",
      bytes: 2,
      headers: [{name: "content-type", value: "text/plain"}],
      url: "https://example.test/",
      body: b"ok",
    }),
  )?

  let response = net.request({method: "GET", url: "https://example.test/"})?
  response.body == b"ok"
  let calls = test.calls(ctx, "net.request")
  test.eq(calls[0].args.method, "GET")?
}

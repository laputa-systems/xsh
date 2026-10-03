test test_pass {
  assert 1 == 1
}

test test_guard_failure_controls_enclosing_loop {
  var numbers = []
  for source in ["1", "invalid", "2"] {
    guard let number = source.parse_int() else { |_|
      continue
    }

    numbers += [number]
  }

  assert numbers == [1, 2]

  numbers = []
  for source in ["1", "invalid", "2"] {
    guard let number = source.parse_int() else { |_|
      break
    }

    numbers += [number]
  }

  assert numbers == [1]
}

test test_typed_integer_augmented_assignment_keeps_results_and_errors { |ctx|
  var value: Int = 13
  value += 5
  value -= 2
  value *= 3
  value /= 6
  value %= 3
  assert value == 2

  let overflow = test.run_script(
    ctx,
    """var value: Int = 9223372036854775807
value += 1
""",
  )?
  assert ! overflow.success, overflow.stderr
  assert "integer-overflow" in overflow.stderr

  let division = test.run_script(
    ctx,
    """var value: Int = 8
value /= 0
""",
  )?
  assert ! division.success, division.stderr
  assert "division-by-zero" in division.stderr
}

pure sibling_branch_value(choice: Str) -> Int {
  if choice == "first" {
    let value = 0
    value
  } else if choice == "second" {
    let value = 10
    value
  } else {
    let value = 20
    value
  }
}

test test_sibling_if_branches_keep_their_own_local_bindings {
  assert sibling_branch_value("first") == 0
  assert sibling_branch_value("second") == 10
  assert sibling_branch_value("other") == 20
  let choice = "second"
  if choice == "first" {
    let value = 0
    assert value == 0
  } else if choice == "second" {
    let value = 10
    assert value == 10
  }
}

test test_repeated_if_branches_select_statement_and_expression_arms {
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
    assert label == ["first", "second", "third"][value % 3]
  }

  assert total == 222
}

pure locally_selected_arguments(argv: List[Str]) -> List[Str] {
  if argv.len() > 0 and argv[0] == "--" { [] } else { argv }
}

test test_local_args_shadows_predeclared_script_arguments {
  assert locally_selected_arguments(["unknown"]) == ["unknown"]
  assert locally_selected_arguments([]) == []
  let argv = ["unknown"]
  let selected = if argv.len() > 0 and argv[0] == "--" { [] } else { argv }
  assert selected == ["unknown"]
}

test test_skip {
  test.skip("later")
}

test test_temp { |ctx|
  let one = test.temp_path(ctx)
  let two = test.temp_path(ctx)
  assert one != two
  let file = test.temp_file(ctx, name: "data", contents: b"ok")?
  let data = fs.read_text(file)?
  assert data == "ok"
}

test test_process_command_builder {
  let command = process.command {
    run true
  }

  let status = process.run(command)?
  assert status.exited_with(0), "builder command should run"
}

pure language_sugar_label(value: Str) -> Result[Str] {
  value
}

pure language_sugar_returned(value: Str) -> Result[Str] {
  return value when value != ""
  value
}

test test_language_sugar_edge_cases { |ctx|
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

  assert language_sugar_label("ok")? == "ok"
  assert language_sugar_returned("return")? == "return"
  assert content == """alpha
beta
"""
  assert raw == r"\n ${literal}"
  assert nested == "demo:x}:1"
  assert escaped == "\${not_interp}:ok:ca"
  assert names[0] == "note.txt"
}

test test_dns_mock { |ctx|
  test.mock(
    ctx,
    "dns.lookup",
    {name: "example.test"},
    Ok([{name: "example.test", record: "A", value: "127.0.0.1", ttl: 60}]),
  )?

  let records = dns.lookup("example.test")?
  assert records[0].value == "127.0.0.1"
  let calls = test.calls(ctx, "dns.lookup")
  assert calls.len() == 1
}

test test_net_mock { |ctx|
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
  assert response.body == b"ok"
  let calls = test.calls(ctx, "net.request")
  assert calls[0].args.get("method")?.require(Str)? == "GET"
}

test module_command_fsync_runs_the_checked_operation { |ctx|
  let result = test.run_script(
    ctx,
    r"""let scratch = fs.tempdir()?
defer scratch.close()
let durable = fp"{scratch.host_path()?}/durable"
durable.write("persisted")
fs.fsync $durable
fs.fsync $durable ?
print durable.read_text()?
""",
  )?
  assert result.success, result.stderr
  assert result.stdout == "persisted\n", result.stdout
}

test module_command_scalar_words_defaults_and_flags { |ctx|
  let result = test.run_script(ctx, r"""let scratch = fs.tempdir()?
defer scratch.close()
let durable = fp"{scratch.host_path()?}/payload.json"
json.write $durable ({value: 1}) false
assert durable.read_text()? == "{\"value\":1}"
json.write $durable --pretty ({value: 1})
assert "\n" in durable.read_text()?
json.write $durable ({value: 1}) --pretty
assert "\n" in durable.read_text()?
json.write $durable ("--pretty")
assert durable.read_text()? == "\"--pretty\""
let payload_text = "--pretty"
json.write $durable $payload_text
assert durable.read_text()? == "\"--pretty\""
time.sleep 0ms
time.sleep ${0}ms
let pause = 0ms
time.sleep $pause
print completed
""")?
  assert result.success, result.stderr
  assert result.stdout == "completed\n", result.stdout
}

test module_command_operands_run_once_in_source_order { |ctx|
  let result = test.run_script(ctx, r"""type Payload = {value: Int}
proc destination(durable: Path) [] -> Path {
  print destination
  durable
}
proc payload() [] -> Payload {
  print payload
  {value: 7}
}
let scratch = fs.tempdir()?
defer scratch.close()
let durable = fp"{scratch.host_path()?}/ordered.json"
json.write (destination(durable)) --pretty (payload())
assert "7" in durable.read_text()?
print completed
""")?
  assert result.success, result.stderr
  assert result.stdout == "destination\npayload\ncompleted\n", result.stdout
}

test module_command_conversion_failure_stops_before_host_call { |ctx|
  let durable = test.temp_file(ctx, name: "untouched.json", contents: b"untouched")?
  let result = test.run_script(ctx, r"""type Payload = {value: Int}
proc destination(durable: Path) [] -> Path {
  print destination
  durable
}
proc payload() [] -> Payload {
  print payload
  {value: 7}
}
json.write (destination(Path(args[0]))) (payload()) maybe
print unreachable
""", [durable])?
  assert !result.success
  assert result.stdout == "destination\npayload\n", result.stdout
  assert "command argument `pretty` expects Bool" in result.stderr, result.stderr
  assert "maybe" in result.stderr, result.stderr
  assert ":10:53" in result.stderr, result.stderr
  assert durable.read_text()? == "untouched"
}

test module_command_failure_propagates_without_question_mark { |ctx|
  let durable = test.temp_file(ctx, name: "missing", contents: b"discard")?
  durable.remove()
  let result = test.run_script(ctx, r"""fs.fsync (Path(args[0]))
print unreachable
""", [durable])?
  assert !result.success
  assert result.stdout == "", result.stdout
  assert "fs-fsync" in result.stderr, result.stderr
}

test module_command_check_failure_prevents_operand_execution { |ctx|
  for case in [
    {source: r"""print unreachable
proc denied() [] { fs.fsync missing }
""", code: "check.effect-violation"},
    {source: r"""print unreachable
fs.fsync (false)
""", code: "check.type-mismatch"},
    {source: r"""print unreachable
json.write missing ({value: 1}) --unknown
""", code: "check.module-command-flag"},
    {source: r"""print unreachable
json.write missing ({value: 1}) --pretty --pretty
""", code: "check.module-command-flag"},
  ] {
    let result = test.run_script(ctx, case.source)?
    assert !result.success, case.source
    assert result.stdout == "", result.stdout
    assert case.code in result.stderr, result.stderr
  }
}

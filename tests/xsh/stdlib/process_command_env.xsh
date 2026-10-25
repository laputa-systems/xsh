test test_command_argv_byte_environment_overlays_inherited_entries { |ctx|
  let result = test.expect(ctx, r"""let environment: Map[Str, Bytes] = {"RAW": b"a\x80b", "REPLACED": b"new"}
env INHERITED="parent" REPLACED="old" {
  let command = process.command_argv(p"/bin/sh", ["sh", "-c", "printf '%s|%s|' \"$INHERITED\" \"$REPLACED\"; printf %s \"$RAW\" | od -An -tx1"], env: environment)
  assert process.run(command)?.ok
}
""", status: 0)?
  assert result.stdout.trim() == "parent|new| 61 80 62"
}

test test_command_argv_byte_environment_rejects_invalid_names_and_nul { |ctx|
  for index in [0, 1, 2] {
    let result = test.expect(ctx, r"""let names = ["", "A=B", "A\0B"]
let name = names[args[0].parse_int()?]
let environment: Map[Str, Bytes] = {[name]: b"value"}
let command = process.command_argv("true", ["true"], env: environment)
let _ = process.run(command)?
""", status: 3, args: [f"{index}"], stderr: ["env-name"])?
    assert result.stdout == ""
  }
  let result = test.expect(ctx, r"""let environment: Map[Str, Bytes] = {"VALID": b"a\0b"}
let command = process.command_argv("true", ["true"], env: environment)
let _ = process.run(command)?
""", status: 3, stderr: ["env-value"])?
  assert result.stdout == ""
}

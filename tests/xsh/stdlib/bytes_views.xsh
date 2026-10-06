# Byte slices retain their type when forwarded through a defaulted script
# parameter and converted into owned stdin at the child-process boundary.
proc view_input_command(target: Path, argv: List[Str], directory: Path, output: Path, errors: Path, input = b"") [process] -> Command {
  process.command_argv(target, argv, directory, {}, input, output, errors)
}

test test_bytes_view_is_command_stdin_not_a_file_path { |ctx|
  let data = b"prefix\0\xffsuffix"
  let command = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), "--version"], stdin: data[6..8])
  let _ = command
}

test test_forwarded_bytes_view_preserves_binary_child_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "byte-view-command")?
  let child = fp"{root}/child.xsh"
  child.write("io.write_stdout_bytes(io.stdin_bytes()?)?\n")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let data = b"prefix\0\xffsuffix"
  let command = view_input_command(ctx.xsh_bin, [ctx.xsh_bin.display(), child.display()], root, out, err, data[6..8])
  let result = process.run(command)?
  assert result.exit_code()? == 0
  assert out.read_bytes()? == b"\0\xff"
  assert err.read_text()? == ""
}

test test_bytes_view_command_builder_uses_input_redirection { |ctx|
  let data = b"prefix\0\xffsuffix"
  let command = process.command {
    stdin = data[6..8]
    run ${ctx.xsh_bin} --version
  }
  let _ = command
}

test test_empty_bytes_view_is_empty_child_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "empty-byte-view")?
  let child = fp"{root}/child.xsh"
  child.write("io.write_stdout_bytes(io.stdin_bytes()?)?\n")
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let data = b"retained backing data"
  let command = view_input_command(ctx.xsh_bin, [ctx.xsh_bin.display(), child.display()], root, out, err, data[4..4])
  let result = process.run(command)?
  assert result.exit_code()? == 0
  assert out.read_bytes()? == b""
  assert err.read_text()? == ""
}

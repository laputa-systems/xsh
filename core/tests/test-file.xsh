type Ran = {status: Int, stdout: Str, stderr: Str}

proc invoke(ctx: TestContext, args: List[Str], input = b"", vars: Record = {LC_ALL: "C", TERM: "xterm"}, cwd: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "file-capture")?
  let out = fp"{root}/out"
  let err = fp"{root}/err"
  let script = fp"{ctx.core_dir}/file.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), cwd ?? root, vars, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8()?, stderr: err.read_text()?})
}

test test_file_magic_and_mime { |ctx|
  let png = test.temp_file(ctx, name: "image.dat", contents: b"\x89PNG\r\n\x1a\n")?
  assert invoke(ctx, ["-b", png.display()])?.stdout == "PNG image data\n"
  assert invoke(ctx, ["-bi", png.display()])?.stdout == "image/png; charset=binary\n"
  let text = test.temp_file(ctx, name: "plain", contents: b"hello\n")?
  assert invoke(ctx, ["--mime-type", text.display()])?.stdout.ends_with("text/plain\n")
}

test test_file_stdin_and_empty { |ctx|
  assert invoke(ctx, ["-b", "-"], b"#!/bin/xsh\n")?.stdout == "/bin/xsh script, ASCII text executable\n"
  let empty = test.temp_file(ctx, name: "empty")?
  assert invoke(ctx, ["-b", empty.display()])?.stdout == "empty\n"
}

test test_file_symlink_and_error_status { |ctx|
  let root = test.temp_dir(ctx, name: "file-links")?
  let plain = fp"{root}/plain"
  plain.write("hello\n")
  let link = fp"{root}/link"
  link.symlink(to: plain)
  assert invoke(ctx, ["-b", link.display()])?.stdout.starts_with("symbolic link to ")
  assert invoke(ctx, ["-bL", link.display()])?.stdout == "ASCII text\n"
  let missing = fp"{root}/missing"
  assert invoke(ctx, [missing.display()])?.status == 0
  assert invoke(ctx, ["-E", missing.display()])?.status == 1
}

test test_file_archive_and_binary_magic { |ctx|
  assert invoke(ctx, ["-bi", "-"], b"\x1f\x8b\x08\0")?.stdout == "application/gzip; charset=binary\n"
  assert invoke(ctx, ["-b", "-"], b"hello\0binary")?.stdout == "data\n"
  assert invoke(ctx, ["-b", "-"], b"{\"field\":1}\n")?.stdout == "JSON text data\n"
}

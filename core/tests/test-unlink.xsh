test test_unlink_removes_link_and_refuses_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let target = fp"{root}/target"
  target.write("keep")
  let link = fp"{root}/link"
  link.symlink(to: target)
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $link
  assert target.read_text()? == "keep"
  assert ! link.exists()?
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $root
  assert status.exited_with(1)
  assert root.is_dir()?
}

test test_unlink_accepts_non_utf8_operand { |ctx|
  let root = test.temp_dir(ctx, name: "unlink-raw-path")?
  let target = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  target.write("remove")
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/unlink.xsh"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"--", target], root, {LC_ALL: "C"}, b"", stdout, stderr))?
  assert status.exited_with(0), stderr.read_text()?
  assert fs.stat(target) is Err(_)
  let missing_status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, p"--", script, p"--", target], root, {LC_ALL: "C"}, b"", stdout, stderr))?
  let missing_error = stderr.read_text()?
  assert missing_status.exited_with(1)
  assert r"\377" in missing_error
  assert "No such file or directory" in missing_error
}

test test_unlink_extra_operand_is_not_removed { |ctx|
  let root = test.temp_dir(ctx)?
  let a = fp"{root}/a"
  let b = fp"{root}/b"
  a.write("a")
  b.write("b")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $a $b
  assert status.exited_with(1)
  assert a.exists()? and b.exists()?
}

test test_unlink_extra_operand_shows_usage { |ctx|
  let root = test.temp_dir(ctx)?
  let first = fp"{root}/first"
  let extra = fp"{root}/extra"
  first.write("first")
  extra.write("extra")

  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/unlink.xsh" -- $first $extra
  assert result.status.exited_with(1)
  assert "Usage: unlink FILE" in result.stderr
  assert first.exists()? and extra.exists()?
}

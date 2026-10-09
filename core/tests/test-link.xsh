type Ran = {status: Int, stdout: Str, stderr: Str}

proc link_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "link")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/link.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?.exit_code()?
  Ok({status: status, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_link {
  |ctx|
  let src = test.temp_file(ctx, name: "source.txt", contents: b"same\n")?
  let dst = test.temp_path(ctx, name: "linked.txt")
  let result = link_run(ctx, [src.display(), dst.display()])?
  assert result.status == 0
  assert result.stderr == ""

  assert dst.read_text()? == """same
"""
  assert fs.stat(src)?.ino == fs.stat(dst)?.ino
}

test test_link_operand_diagnostics { |ctx|
  let missing = link_run(ctx, [])?
  assert missing.status == 1
  assert missing.stderr == "link: missing operand\nTry 'link --help' for more information.\n"

  let after = link_run(ctx, ["source"])?
  assert after.status == 1
  assert after.stderr == "link: missing operand after 'source'\nTry 'link --help' for more information.\n"

  let extra = link_run(ctx, ["a", "b", "c"])?
  assert extra.status == 1
  assert extra.stderr == "link: extra operand 'c'\nTry 'link --help' for more information.\n"
}

test test_link_existing_destination_and_missing_source { |ctx|
  let root = test.temp_dir(ctx, name: "link-errors")?
  let src = fp"{root}/source"
  let dest = fp"{root}/dest"
  src.write("source")
  dest.write("old")

  let exists = link_run(ctx, [src.display(), dest.display()])?
  assert exists.status == 1
  assert exists.stderr == f"link: cannot create link '{dest}' to '{src}': File exists\n"
  assert dest.read_text()? == "old"

  let missing = link_run(ctx, [f"{root}/missing", f"{root}/missing-link"])?
  assert missing.status == 1
  assert "cannot create link" in missing.stderr
  assert "No such file or directory" in missing.stderr
}

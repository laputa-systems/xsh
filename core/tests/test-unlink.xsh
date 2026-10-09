type Ran = {status: Int, stdout: Str, stderr: Str}

proc unlink_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "unlink")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/unlink.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?.exit_code()?
  Ok({status: status, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_unlink_removes_files_and_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "unlink-paths")?
  let file = fp"{root}/file"
  file.write("contents")
  let removed = unlink_run(ctx, [file.display()])?
  assert removed.status == 0
  assert ! file.exists()?

  let target = fp"{root}/target"
  target.write("target")
  let link = fp"{root}/link"
  fs.symlink(target, link)
  let result = unlink_run(ctx, [link.display()])?
  assert result.status == 0
  assert ! link.exists()?
  assert target.read_text()? == "target"
}

test test_unlink_rejects_directories_and_missing_operands { |ctx|
  let root = test.temp_dir(ctx, name: "unlink-errors")?
  let dir = fp"{root}/dir"
  dir.mkdir()

  let directory = unlink_run(ctx, [dir.display()])?
  assert directory.status == 1
  assert directory.stderr == f"unlink: cannot unlink '{dir}': Is a directory\n"
  assert dir.exists()?

  let missing = unlink_run(ctx, [f"{root}/missing"])?
  assert missing.status == 1
  assert "cannot unlink" in missing.stderr
  assert "No such file or directory" in missing.stderr
}

test test_unlink_operand_diagnostics_and_options { |ctx|
  let missing = unlink_run(ctx, [])?
  assert missing.status == 1
  assert missing.stderr == "unlink: missing operand\nTry 'unlink --help' for more information.\n"

  let extra = unlink_run(ctx, ["one", "two"])?
  assert extra.status == 1
  assert extra.stderr == "unlink: extra operand 'two'\nTry 'unlink --help' for more information.\n"

  let option = unlink_run(ctx, ["-f", "file"])?
  assert option.status == 1
  assert option.stderr == "unlink: invalid option -- 'f'\nTry 'unlink --help' for more information.\n"
}

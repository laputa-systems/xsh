type PermRan = {status: Int, stdout: Str, stderr: Str}

proc perm_run(ctx: TestContext, args: List[Union[Str, Path]]) [fs, process, error] -> Result[PermRan] {
  let root = test.temp_dir(ctx, name: "capture")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let words: List[Union[Str, Path]] = collect { yield ctx.xsh_bin; yield fp"{ctx.core_dir}/chgrp.xsh"; yield "--"; for arg in args { yield arg } }
  let status = process.run(process.command_argv(ctx.xsh_bin, words, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err))?
  {status: status.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_chgrp_current_group { |ctx|
  let target = test.temp_file(ctx, name: "grouped.txt", contents: b"payload")?
  let current = group.current()?
  let name = current.name
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chgrp.xsh" -- $name $target
  assert output == ""
  assert target.metadata()?.gid == current.gid
  let root = test.temp_dir(ctx, name: "grouped-tree")?
  let child = fp"{root}/child.txt"
  child.write("payload")
  let recursive = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/chgrp.xsh" -- -R $name $root
  assert recursive == ""
  assert child.metadata()?.gid == current.gid
}

test test_chgrp_numeric_reference_and_changes { |ctx|
  let file = test.temp_file(ctx, name: "group", contents: b"x")?
  let gid = fs.stat(file)?.gid
  let result = perm_run(ctx, ["-v", f"+{gid}", file.display()])?
  assert result.status == 0
  assert result.stdout.find("group of") != null
  assert result.stdout.find("retained as") != null
  assert perm_run(ctx, ["-c", f"--reference={file}", file.display()])?.stdout == ""
  assert perm_run(ctx, ["--from=99999", "12345", file.display()])?.status == 0
  assert fs.stat(file)?.gid == gid
}

test test_chgrp_failure_status_survives_successful_operand { |ctx|
  let target = test.temp_file(ctx, name: "group-target", contents: b"x")?
  let gid = fs.stat(target)?.gid
  let result = perm_run(ctx, ["--qui", f"{gid}", f"{target}/missing", target.display()])?
  assert result.status == 1
  assert result.stdout == ""
  assert result.stderr == ""
}

test test_chgrp_empty_group_preserves_identity { |ctx|
  let target = test.temp_file(ctx, name: "no-group-change", contents: b"x")?
  let before = fs.stat(target)?
  let result = perm_run(ctx, ["-v", "", target.display()])?
  assert result.status == 0, result.stderr
  assert fs.stat(target)?.gid == before.gid
  assert result.stdout == f"ownership of '{target}' retained\n"
}

test test_chgrp_validates_from_before_group_operand { |ctx|
  let target = test.temp_file(ctx, name: "from-user", contents: b"x")?
  let result = perm_run(ctx, ["--from", "xsh-missing-user", "xsh-missing-group", target.display()])?
  assert result.status == 1
  assert result.stderr == "chgrp: invalid user: 'xsh-missing-user'\n"
}

test test_chgrp_accepts_non_utf8_operand_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "raw-group-operand")?
  let file = Path.parse_bytes(bytes.concat([root.bytes(), b"/file\xff"]))?
  file.write("x")
  let current = group.current()?
  let result = perm_run(ctx, [current.name, file])?
  assert result.status == 0, result.stderr
  assert fs.stat(file)?.gid == current.gid
}

use core.lib.acl

type AclOutput = {status: Int, stdout: Str, stderr: Str}
proc get_acl_run(ctx: TestContext, root: Path, args: List[Str]) [fs, process, error] -> Result[AclOutput] {
  let out = fp"{root}/stdout"; let err = fp"{root}/stderr"; let script = fp"{ctx.core_dir}/getfacl.xsh"
  let child = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "--"].extend(args), root, {LC_ALL: "C"}, b"", out, err)
  let result = process.run(child)?
  {status: result.exit_code()?, stdout: out.read_text()?, stderr: err.read_text()?}
}

test test_getfacl_headers_selection_table_and_follow_policy { |ctx|
  let root = test.temp_dir(ctx, name: "getfacl")?
  let file = fp"{root}/file"; file.write("content"); file.chmod(0o640)
  fp"{root}/link".symlink(to: p"file")
  let output = get_acl_run(ctx, root, ["-n", "file"])?
  assert output.status == 0, output.stderr
  assert output.stdout.starts_with("# file: file\n# owner:")
  assert "user::rw-\ngroup::r--\nother::---\n" in output.stdout
  assert get_acl_run(ctx, root, ["-P", "link"])?.stdout == ""
  assert "user::rw-" in get_acl_run(ctx, root, ["-c", "link"])?.stdout
  assert get_acl_run(ctx, root, ["-cs", "file"])?.stdout == ""
  assert get_acl_run(ctx, root, ["-cd", "file"])?.stdout == "\n"
  assert "USER" in get_acl_run(ctx, root, ["-ctn", "file"])?.stdout
  let absolute = get_acl_run(ctx, root, [file.display()])?
  assert "Removing leading '/'" in absolute.stderr
  assert get_acl_run(ctx, root, ["-p", file.display()])?.stderr == ""
}

test test_getfacl_recursive_cycles_fail_without_following_physical_links { |ctx|
  let root = test.temp_dir(ctx, name: "getfacl-cycle")?
  let dir = fp"{root}/dir"; dir.mkdir()
  fp"{dir}/file".write("content"); fp"{dir}/cycle".symlink(to: p".")
  assert get_acl_run(ctx, root, ["-RLP", "dir"])?.status == 0
  assert get_acl_run(ctx, root, ["-RL", "dir"])?.status == 1
}

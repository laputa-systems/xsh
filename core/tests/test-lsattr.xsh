type FakeOutput = {success: Bool, status: Int, stdout: Str, stderr: Str, stdout_bytes: Bytes, stderr_bytes: Bytes}

proc lsattr_fake(ctx: TestContext, args: List[Str], flags: Int, generation: Int = 7, project: Int = 19) [fs, process, error] -> FakeOutput {
  test.linux_fake(ctx, {file_attrs_flags: flags, file_version: generation, file_project: project})
  let source = fp"{ctx.core_dir}/lsattr.xsh".read_text()?
  test.run_script(ctx, source, args, {XSH_MODULE_PATH: ctx.core_dir.display()}, b"", "lsattr")?
}

test test_lsattr_compact_long_and_generation_output { |ctx|
  let root = test.temp_dir(ctx, name: "lsattr")?
  let file = fp"{root}/file"
  file.write("content")
  let compact = lsattr_fake(ctx, [file.display()], 64)
  assert compact.status == 0, compact.stderr
  assert compact.stdout == "------d--------------- " + file.display() + "\n"
  let detailed = lsattr_fake(ctx, ["-l", file.display()], 64)
  assert detailed.status == 0
  assert detailed.stdout.ends_with(" No_Dump\n")
  let generation = lsattr_fake(ctx, ["-v", file.display()], 0)
  assert generation.status == 0
  assert generation.stdout.starts_with("7          ")
}

test test_lsattr_directory_filters_and_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "lsattr-dir")?
  fp"{root}/visible".write("content")
  fp"{root}/.hidden".write("content")
  let plain = lsattr_fake(ctx, [root.display()], 0)
  assert plain.status == 0, plain.stderr
  assert "visible" in plain.stdout and ".hidden" not in plain.stdout
  let all = lsattr_fake(ctx, ["-a", root.display()], 0)
  assert ".hidden" in all.stdout
  let directory = lsattr_fake(ctx, ["-d", root.display()], 0)
  assert "visible" not in directory.stdout
  fp"{root}/link".symlink(to: p"visible")
  let link = lsattr_fake(ctx, [fp"{root}/link".display()], 0)
  assert link.status == 1
  assert "Operation not supported" in link.stderr
}


test test_lsattr_preserves_operand_order_and_recursive_headings { |ctx|
  let root = test.temp_dir(ctx, name: "lsattr-order")?
  fp"{root}/dir/subdir".mkdir(parents: true)
  fp"{root}/dir/subdir/inner".write("content")
  fp"{root}/after".write("content")
  let output = lsattr_fake(ctx, ["-R", fp"{root}/dir".display(), fp"{root}/after".display()], 0)
  assert output.status == 0, output.stderr
  let heading = output.stdout.find(fp"{root}/dir/subdir".display() + ":") ?? -1
  let nested = output.stdout.find(fp"{root}/dir/subdir/inner".display()) ?? -1
  let last = output.stdout.find(fp"{root}/after".display()) ?? -1
  assert heading >= 0 and nested > heading and last > nested
}


test test_lsattr_project_ids_and_generation_columns { |ctx|
  let root = test.temp_dir(ctx, name: "lsattr-project")?
  let file = fp"{root}/file"
  file.write("content")
  let output = lsattr_fake(ctx, ["-p", "-v", file.display()], 64, project: 4294967295)
  assert output.status == 0, output.stderr
  assert output.stdout.starts_with("4294967295 7          ------d")
}

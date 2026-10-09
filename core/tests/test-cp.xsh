type CpRun = {status: Int, stdout: Str, stderr: Str}

proc cp_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[CpRun] {
  let root = test.temp_dir(ctx, name: "cp-run")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/cp.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, b"", out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_cp_file_copy_and_verbose { |ctx|
  let root = test.temp_dir(ctx, name: "cp-file")?
  let src = fp"{root}/source.txt"
  let dst = fp"{root}/dest.txt"
  src.write("hello")

  let result = cp_run(ctx, [src.display(), dst.display()])?
  assert result.status == 0
  assert result.stdout == ""
  assert result.stderr == ""
  assert dst.read_text()? == "hello"

  let verbose = cp_run(ctx, ["-v", src.display(), dst.display()])?
  assert verbose.status == 0
  assert verbose.stdout == "'" + src.display() + "' -> '" + dst.display() + "'\n", verbose.stdout
  assert verbose.stderr == ""
}

test test_cp_recursive_directory_copy { |ctx|
  let root = test.temp_dir(ctx, name: "cp-recursive")?
  let src = fp"{root}/source"
  src.mkdir()
  fp"{src}/nested.txt".write("nested")
  let dst = fp"{root}/copy"

  let result = cp_run(ctx, ["-R", src.display(), dst.display()])?
  assert result.status == 0, result.stderr
  assert fp"{dst}/nested.txt".read_text()? == "nested"
}

test test_cp_no_clobber_preserves_destination { |ctx|
  let root = test.temp_dir(ctx, name: "cp-no-clobber")?
  let src = fp"{root}/source"
  let dst = fp"{root}/dest"
  src.write("new")
  dst.write("old")

  let result = cp_run(ctx, ["--no-clobber", src.display(), dst.display()])?
  assert result.status == 0
  assert dst.read_text()? == "old"
}

test test_cp_dereference_and_preserve_symbolic_links { |ctx|
  let root = test.temp_dir(ctx, name: "cp-symlinks")?
  let target = fp"{root}/target"
  let source_link = fp"{root}/source-link"
  target.write("through the link")
  fs.symlink(target, source_link)?

  let followed = fp"{root}/followed"
  let copy = cp_run(ctx, [source_link.display(), followed.display()])?
  assert copy.status == 0, copy.stderr
  assert followed.read_text()? == "through the link"

  let preserved = fp"{root}/preserved"
  let no_deref = cp_run(ctx, ["-P", source_link.display(), preserved.display()])?
  assert no_deref.status == 0, no_deref.stderr
  assert preserved.readlink()? == target
}

test test_cp_recursive_link_follow_modes { |ctx|
  let root = test.temp_dir(ctx, name: "cp-follow-modes")?
  let source = fp"{root}/source"
  source.mkdir()
  fp"{source}/target".write("recursive target")
  fs.symlink(p"target", fp"{source}/link")?

  let preserved = fp"{root}/preserved"
  let preserve_result = cp_run(ctx, ["-R", "-P", source.display(), preserved.display()])?
  assert preserve_result.status == 0, preserve_result.stderr
  assert fp"{preserved}/link".readlink()? == p"target"

  let followed = fp"{root}/followed"
  let follow_result = cp_run(ctx, ["-R", "-L", source.display(), followed.display()])?
  assert follow_result.status == 0, follow_result.stderr
  assert fp"{followed}/link".read_text()? == "recursive target"
}

test test_cp_backup_and_update { |ctx|
  let root = test.temp_dir(ctx, name: "cp-backup")?
  let src = fp"{root}/source"
  let dst = fp"{root}/dest"
  src.write("new")
  dst.write("old")

  let result = cp_run(ctx, ["--backup=simple", "--suffix=.bak", src.display(), dst.display()])?
  assert result.status == 0, result.stderr
  assert dst.read_text()? == "new"
  assert fp"{dst}.bak".read_text()? == "old"

  let skipped = cp_run(ctx, ["--update=none", src.display(), dst.display()])?
  assert skipped.status == 0
  assert dst.read_text()? == "new"
}

test test_cp_same_file_backup_and_option_end_marker { |ctx|
  let root = test.temp_dir(ctx, name: "cp-same-file")?
  let source = fp"{root}/source"
  source.write("same")

  let same = cp_run(ctx, ["--backup=simple", "--force", source.display(), source.display()])?
  assert same.status == 0, same.stderr
  assert source.read_text()? == "same"
  assert fp"{source}~".read_text()? == "same"

  let destination = fp"{root}/copy"
  let marked = cp_run(ctx, ["--", source.display(), destination.display()])?
  assert marked.status == 0, marked.stderr
  assert destination.read_text()? == "same"
}

test test_cp_copy_contents_reads_special_files { |ctx|
  let root = test.temp_dir(ctx, name: "cp-copy-contents")?
  let source = p"/dev/null"
  let destination = fp"{root}/destination"

  let result = cp_run(ctx, ["-R", "--copy-contents", source.display(), destination.display()])?
  assert result.status == 0, result.stderr
  assert result.stderr == ""
  match fs.stat(destination) {
    Ok(meta) => assert meta.kind == "file"
    Err(failure) => assert false, failure.message
  }
  assert destination.read_bytes()? == b""
}

test test_cp_attributes_only_preserves_special_file_type { |ctx|
  let root = test.temp_dir(ctx, name: "cp-attributes-fifo")?
  let source = fp"{root}/source"
  let destination = fp"{root}/destination"
  fs.mkfifo(source, 0o600)?

  let result = cp_run(ctx, ["--attributes-only", source.display(), destination.display()])?
  assert result.status == 0, result.stderr
  assert fs.stat(destination)?.kind == "fifo"
}

test test_cp_help_and_missing_operand { |ctx|
  let help = cp_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stdout.starts_with("Usage: cp "), help.stdout
  assert help.stderr == ""

  let missing = cp_run(ctx, [])?
  assert missing.status == 1
  assert missing.stdout == ""
  assert missing.stderr == "cp: missing file operand\nTry 'cp --help' for more information.\n", missing.stderr
}

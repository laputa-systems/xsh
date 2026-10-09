type Ran = {status: Int, stdout: Str, stderr: Str}

proc mv_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/mv.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?.exit_code()?
  Ok({status: status, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_mv_file_and_target_directory { |ctx|
  let root = test.temp_dir(ctx, name: "mv")?
  let src = fp"{root}/src.txt"
  src.write("hello")
  let dir = fp"{root}/dir"
  dir.mkdir()

  let result = mv_run(ctx, root, ["-t", dir.display(), src.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert ! src.exists()?
  assert fp"{dir}/src.txt".read_text()? == "hello"
}

test test_mv_no_clobber_force_and_verbose { |ctx|
  let root = test.temp_dir(ctx, name: "mv-overwrite")?
  let src = fp"{root}/src"
  let dst = fp"{root}/dst"
  src.write("new")
  dst.write("old")

  let no_clobber = mv_run(ctx, root, ["-n", src.display(), dst.display()])?
  assert no_clobber.status == 0
  assert dst.read_text()? == "old"
  assert src.read_text()? == "new"

  let forced = mv_run(ctx, root, ["-n", "-f", "-v", src.display(), dst.display()])?
  assert forced.status == 0
  assert dst.read_text()? == "new"
  assert ! src.exists()?
  assert forced.stdout == f"'{src}' -> '{dst}'\n"
}

test test_mv_interactive_decline_preserves_directory_and_destination { |ctx|
  let root = test.temp_dir(ctx, name: "mv-interactive-decline-dir")?
  let source = fp"{root}/source-dir"
  let destination = fp"{root}/destination-file"
  source.mkdir()
  destination.write("keep")

  let result = mv_run(ctx, root, ["-i", source.display(), destination.display()], b"n\n")?
  assert result.status == 1
  assert source.exists()?
  assert destination.read_text()? == "keep"
}

test test_mv_update_older_and_backup { |ctx|
  let root = test.temp_dir(ctx, name: "mv-update")?
  let src = fp"{root}/src"
  let dst = fp"{root}/dst"
  src.write("older")
  dst.write("newer")
  fs.set_times(src, atime_ns: null, mtime_ns: 1000000000)?
  fs.set_times(dst, atime_ns: null, mtime_ns: 2000000000)?

  let skipped = mv_run(ctx, root, ["--update=older", src.display(), dst.display()])?
  assert skipped.status == 0
  assert src.read_text()? == "older"
  assert dst.read_text()? == "newer"

  let replaced = mv_run(ctx, root, ["--backup=numbered", "--update=all", src.display(), dst.display()])?
  assert replaced.status == 0
  assert dst.read_text()? == "older"
  assert fp"{dst}.~1~".read_text()? == "newer"
}

test test_mv_operand_and_option_diagnostics { |ctx|
  let root = test.temp_dir(ctx, name: "mv-usage")?
  let missing = mv_run(ctx, root, [])?
  assert missing.status == 1
  assert missing.stderr == "mv: missing file operand\nTry 'mv --help' for more information.\n"

  let one = mv_run(ctx, root, ["source"])?
  assert one.status == 1
  assert one.stderr == "mv: missing destination file operand after 'source'\nTry 'mv --help' for more information.\n"

  let unsupported = mv_run(ctx, root, ["-Z", "a", "b"])?
  assert unsupported.status == 1
  assert "SELinux contexts are not available" in unsupported.stderr
}

test test_mv_end_of_options_belongs_to_applet { |ctx|
  let root = test.temp_dir(ctx, name: "mv-double-dash")?
  let src = fp"{root}/-source"
  let dst = fp"{root}/destination"
  src.write("contents")
  let result = mv_run(ctx, root, ["--", "-source", "destination"])?
  assert result.status == 0
  assert dst.read_text()? == "contents"
}

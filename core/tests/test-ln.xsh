type Ran = {status: Int, stdout: Str, stderr: Str}

proc ln_run(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "ln")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/ln.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?.exit_code()?
  Ok({status: status, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

test test_ln_symbolic_force { |ctx|
  let root = test.temp_dir(ctx, name: "ln-force")?
  let src = fp"{root}/src.txt"
  let dst = fp"{root}/dst.txt"
  src.write("new")
  dst.write("old")
  let result = ln_run(ctx, ["-sf", src.display(), dst.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert dst.readlink()?.display() == src.display()
}

test test_ln_target_directory_and_relative_links { |ctx|
  let root = test.temp_dir(ctx, name: "ln-target")?
  let source = fp"{root}/source"
  source.write("data")
  let dir = fp"{root}/dir"
  dir.mkdir()

  let result = ln_run(ctx, ["-s", "-t", dir.display(), source.display()])?
  assert result.status == 0
  assert fp"{dir}/source".readlink()?.display() == source.display()

  let links = fp"{root}/links"
  links.mkdir()
  let relative = ln_run(ctx, ["-sr", source.display(), f"{links}/source"])?
  assert relative.status == 0
  assert fp"{links}/source".readlink()?.display() == "../source"
}

test test_ln_logical_hardlink_follows_source_symlink { |ctx|
  let root = test.temp_dir(ctx, name: "ln-logical")?
  let source = fp"{root}/source"
  source.write("data")
  let link = fp"{root}/source-link"
  fs.symlink(source, link)
  let dest = fp"{root}/dest"

  let result = ln_run(ctx, ["-L", link.display(), dest.display()])?
  assert result.status == 0
  assert fs.stat(dest)?.ino == fs.stat(source)?.ino
}

test test_ln_no_clobber_and_same_file { |ctx|
  let root = test.temp_dir(ctx, name: "ln-no-clobber")?
  let src = fp"{root}/src"
  let dst = fp"{root}/dst"
  src.write("source")
  dst.write("old")

  let exists = ln_run(ctx, [src.display(), dst.display()])?
  assert exists.status == 1
  assert "File exists" in exists.stderr
  assert dst.read_text()? == "old"

  let same = ln_run(ctx, ["-f", src.display(), src.display()])?
  assert same.status == 1
  assert "are the same file" in same.stderr
}

test test_ln_force_same_inode_is_noop { |ctx|
  let root = test.temp_dir(ctx, name: "ln-force-same-inode")?
  let source = fp"{root}/source"
  let destination = fp"{root}/destination"
  source.write("same inode")
  fs.link(source, destination)?

  let result = ln_run(ctx, ["-f", source.display(), destination.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert fs.stat(source)?.ino == fs.stat(destination)?.ino
  assert ! fp"{destination}.xsh-tmp-0".exists()?

  let same_path = ln_run(ctx, ["-f", source.display(), f"{root}/./source"])?
  assert same_path.status == 1
  assert "are the same file" in same_path.stderr
}

test test_ln_usage_diagnostics { |ctx|
  let missing = ln_run(ctx, [])?
  assert missing.status == 1
  assert missing.stderr == "ln: missing operand\nTry 'ln --help' for more information.\n"

  let one = ln_run(ctx, ["source"])?
  assert one.status == 1
  assert one.stderr == "ln: failed to access 'source': No such file or directory\n"

  let relative = ln_run(ctx, ["-r", "source", "dest"])?
  assert relative.status == 1
  assert "--relative is only meaningful with --symbolic" in relative.stderr

  let unsupported = ln_run(ctx, ["-d", "source", "dest"])?
  assert unsupported.status == 1
  assert "directory hard links are not supported" in unsupported.stderr
}

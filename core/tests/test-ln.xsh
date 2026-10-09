type Ran = {status: Int, stdout: Str, stderr: Str}

pure ln_quote(item: Path) -> Str {
  f"'{item}'"
}

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

  let same_path = ln_run(ctx, ["-sf", src.display(), src.display()])?
  assert same_path.status == 1
  assert "are the same file" in same_path.stderr
  assert src.read_text()? == "new"
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

test test_ln_relative_source_symlink_uses_resolved_target { |ctx|
  let root = test.temp_dir(ctx, name: "ln-relative-source")?
  let source = fp"{root}/file1"
  source.write("data")
  fs.symlink(fp"{root}/file1", fp"{root}/file2")

  let result = ln_run(ctx, ["-sr", f"{root}/file2", f"{root}/file3"])?
  assert result.status == 0
  assert fp"{root}/file3".readlink()?.display() == "file1"
}

test test_ln_backup_existing_hardlinks { |ctx|
  let root = test.temp_dir(ctx, name: "ln-backup-hardlinks")?
  let source = fp"{root}/a"
  let destination = fp"{root}/b"
  source.write("data")
  fs.link(source, destination)

  let result = ln_run(ctx, ["--backup", source.display(), destination.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert fs.stat(source)?.ino == fs.stat(destination)?.ino
  assert fp"{root}/b~".read_text()? == "data"

  let same = fp"{root}/same"
  same.write("same")
  let same_path = ln_run(ctx, ["--backup", same.display(), f"{root}/./same"])?
  assert same_path.status == 1
  assert "are the same file" in same_path.stderr
  assert same.read_text()? == "same"
}

test test_ln_backup_suffix_is_safe_and_enables_backup { |ctx|
  let root = test.temp_dir(ctx, name: "ln-backup-suffix")?
  let source = fp"{root}/a"
  let destination = fp"{root}/b"
  source.write("source")
  destination.write("old")

  let result = ln_run(ctx, ["-s", "-S", "_/../c", source.display(), destination.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert destination.readlink()?.display() == source.display()
  assert fp"{root}/b~".read_text()? == "old"
  assert ! fp"{root}/c".exists()?
}

test test_ln_verbose_backup_uses_gnu_order { |ctx|
  let root = test.temp_dir(ctx, name: "ln-verbose-backup")?
  let source = fp"{root}/source"
  let destination = fp"{root}/destination"
  source.write("source")
  destination.write("old")

  let result = ln_run(ctx, ["-s", "-v", "-b", source.display(), destination.display()])?
  assert result.status == 0
  let backup = fp"{destination}~"
  assert result.stdout == f"{ln_quote(backup)} ~ {ln_quote(destination)} -> {ln_quote(source)}\n"
}

test test_ln_no_dereference_destination_symlink_to_directory { |ctx|
  let root = test.temp_dir(ctx, name: "ln-no-dereference-dir")?
  let source = fp"{root}/source"
  source.write("data")
  let directory = fp"{root}/directory"
  directory.mkdir()
  let destination = fp"{root}/destination"
  fs.symlink(directory, destination)

  let refused = ln_run(ctx, ["-n", source.display(), destination.display()])?
  assert refused.status == 1
  assert "failed to create hard link" in refused.stderr
  assert "File exists" in refused.stderr
  assert destination.readlink()?.display() == directory.display()

  let replaced = ln_run(ctx, ["-bn", source.display(), destination.display()])?
  assert replaced.status == 0
  assert fp"{root}/destination~".readlink()?.display() == directory.display()
  assert fs.stat(source)?.ino == fs.stat(destination)?.ino
}

test test_ln_hard_link_directory_diagnostic { |ctx|
  let root = test.temp_dir(ctx, name: "ln-hardlink-directory")?
  let directory = fp"{root}/directory"
  directory.mkdir()

  let result = ln_run(ctx, [directory.display(), fp"{root}/link".display()])?
  assert result.status == 1
  assert "hard link not allowed for directory" in result.stderr
}

test test_ln_duplicate_destination_fails_without_overwriting { |ctx|
  let root = test.temp_dir(ctx, name: "ln-duplicate-destination")?
  let first_dir = fp"{root}/first"
  let second_dir = fp"{root}/second"
  let destination_dir = fp"{root}/destination"
  first_dir.mkdir()
  second_dir.mkdir()
  destination_dir.mkdir()
  fp"{first_dir}/file".write("first")
  fp"{second_dir}/file".write("second")

  let result = ln_run(ctx, [f"{first_dir}/file", f"{second_dir}/file", destination_dir.display()])?
  assert result.status == 1
  assert "File exists" in result.stderr
  assert fp"{destination_dir}/file".read_text()? == "first"
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
  fs.link(source, destination)

  let result = ln_run(ctx, ["-f", source.display(), destination.display()])?
  assert result.status == 0
  assert result.stderr == ""
  assert fs.stat(source)?.ino == fs.stat(destination)?.ino
  assert ! fp"{destination}.xsh-tmp-0".exists()?

  let same_path = ln_run(ctx, ["-f", source.display(), f"{root}/./source"])?
  assert same_path.status == 1
  assert "are the same file" in same_path.stderr

  let symlink_destination = fp"{root}/source-link"
  fs.symlink(source, symlink_destination)
  let replaced = ln_run(ctx, ["-f", source.display(), symlink_destination.display()])?
  assert replaced.status == 0
  assert fs.stat(symlink_destination, follow_symlinks: false)?.kind == "file"
  assert fs.stat(source)?.ino == fs.stat(symlink_destination)?.ino
}

test test_ln_usage_diagnostics { |ctx|
  let missing = ln_run(ctx, [])?
  assert missing.status == 1
  assert missing.stderr == "ln: missing file operand\nTry 'ln --help' for more information.\n"

  let one = ln_run(ctx, ["source"])?
  assert one.status == 1
  assert one.stderr == "ln: failed to access 'source': No such file or directory\n"

  let relative = ln_run(ctx, ["-r", "source", "dest"])?
  assert relative.status == 1
  assert relative.stderr == "ln: cannot do --relative without --symbolic\nTry 'ln --help' for more information.\n"

  let no_destination = ln_run(ctx, ["-s", "-T", "source"])?
  assert no_destination.status == 1
  assert no_destination.stderr == "ln: missing destination file operand after 'source'\nTry 'ln --help' for more information.\n"

  let missing_directory = ln_run(ctx, ["-s", "source", "no-such-directory/link"])?
  assert missing_directory.status == 1
  assert "failed to create symbolic link 'no-such-directory/link': No such file or directory" in missing_directory.stderr

  let unsupported = ln_run(ctx, ["-d", "source", "dest"])?
  assert unsupported.status == 1
  assert "directory hard links are not supported" in unsupported.stderr

  let unsupported_long = ln_run(ctx, ["--directory", "source", "dest"])?
  assert unsupported_long.status == 1
  assert "directory hard links are not supported" in unsupported_long.stderr

  let unsupported_alias = ln_run(ctx, ["-F", "source", "dest"])?
  assert unsupported_alias.status == 1
  assert "directory hard links are not supported" in unsupported_alias.stderr
}

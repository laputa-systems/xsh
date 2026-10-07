test test_ln_symbolic_force { |ctx|
  let root = test.temp_dir(ctx, name: "ln")?
  let src = fp"{root}/src.txt"
  let dst = fp"{root}/dst.txt"
  src.write("new")
  dst.write("old")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sf $src $dst
  assert "src.txt" in dst.readlink()?.display()
}

test test_ln_no_dereference_replaces_directory_symlink { |ctx|
  let root = test.temp_dir(ctx)?
  let directory = fp"{root}/directory"
  directory.mkdir()
  let dest = fp"{root}/dest"
  dest.symlink(to: directory)
  let source = fp"{root}/source"
  source.write("data")
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sfn $source $dest
  assert dest.readlink()? == source
  assert ! fp"{directory}/source".exists()?
}

test test_ln_failed_force_preserves_destination { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/dest"
  dest.write("keep")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -f fp"{root}/missing" $dest
  assert status.exited_with(1)
  assert dest.read_text()? == "keep"
}

test test_ln_relative_dangling_and_physical_hard_links { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/dest"
  let missing = fp"{root}/missing"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sr $missing $dest
  assert dest.readlink()? == p"missing"
  let hard = fp"{root}/hard"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- $dest $hard
  assert fs.stat(hard)?.ino == fs.stat(dest)?.ino
  assert fs.stat(hard)?.kind == "symlink"
}

test test_ln_backup_restored_on_failed_link { |ctx|
  let root = test.temp_dir(ctx)?
  let dest = fp"{root}/dest"
  dest.write("keep")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -b fp"{root}/missing" $dest
  assert status.exited_with(1)
  assert dest.read_text()? == "keep"
  assert ! fp"{dest}~".exists()?
}

test test_ln_force_same_entry_keeps_source { |ctx|
  let source = test.temp_file(ctx, contents: b"keep")?
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sf $source $source
  assert status.exited_with(1)
  assert fs.stat(source)?.kind == "file"
  assert source.read_text()? == "keep"
}

test test_ln_interactive_respects_last_force_option { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let script = fp"{ctx.core_dir}/ln.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let decline = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-sfi", source.display(), dest.display()],
    root, {}, b"n\n", out, err)
  assert process.run(decline)?.exited_with(1)
  assert dest.read_text()? == "old"
  assert err.read_text()? == f"ln: replace '{dest}'? "
  let force = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-sif", source.display(), dest.display()],
    root, {}, b"n\n", out, err)
  assert process.run(force)?.exited_with(0)
  assert dest.readlink()? == source
  assert err.read_text()? == ""
}

test test_ln_abbreviated_option_values_are_not_overwrite_controls { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  source.write("new")
  let directory = fp"{root}/-i"
  directory.mkdir()
  let dest = fp"{directory}/source"
  dest.write("old")
  let script = fp"{ctx.core_dir}/ln.xsh"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), script.display(), "-s", "--for", "--target-d", "-i", source.display()],
    root, {}, b"n\n", fp"{root}/out", fp"{root}/err")
  assert process.run(command)?.exited_with(0)
  assert dest.readlink()? == source
}

test test_ln_relative_resolves_dangling_source_link { |ctx|
  let root = test.temp_dir(ctx)?
  let dangling = fp"{root}/dangling"
  dangling.symlink(to: p"missing")
  let dest = fp"{root}/dest"
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sr $dangling $dest
  assert dest.readlink()? == p"missing"
}

test test_ln_no_dereference_does_not_imply_force { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  dest.write("old")
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -sn $source $dest
  assert status.exited_with(1)
  assert dest.read_text()? == "old"
}

test test_ln_verbose_reports_backup_after_the_link { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let dest = fp"{root}/dest"
  source.write("new")
  let script = fp"{ctx.core_dir}/ln.xsh"
  let simple = run.text ${ctx.xsh_bin} $script -- -s -v $source $dest
  assert simple == f"'{dest}' -> '{source}'\n"

  dest.remove()
  dest.write("old")
  let backed_up = run.text ${ctx.xsh_bin} $script -- -s -v -b $source $dest
  assert backed_up == f"'{dest}' -> '{source}' (backup: '{dest}~')\n"
}

test test_ln_preserves_non_utf8_source_and_destination_names { |ctx|
  let root = test.temp_dir(ctx)?
  let source = Path.parse_bytes(bytes.concat([root.bytes(), b"/source\xff\xfe"]))?
  let hard_link = Path.parse_bytes(bytes.concat([root.bytes(), b"/hard\xff\xfe"]))?
  let symbolic_link = Path.parse_bytes(bytes.concat([root.bytes(), b"/symbolic\xff\xfe"]))?
  source.write("payload")
  let script = fp"{ctx.core_dir}/ln.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let hard_command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, source, hard_link], root, {}, b"", out, err)
  assert process.run(hard_command)?.exited_with(0)
  assert fs.stat(source)?.ino == fs.stat(hard_link)?.ino

  let symbolic_command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, "-s", source, symbolic_link], root, {}, b"", out, err)
  assert process.run(symbolic_command)?.exited_with(0)
  assert symbolic_link.readlink()? == source
}

test test_ln_target_directory_accepts_non_utf8_source_name { |ctx|
  let root = test.temp_dir(ctx)?
  let source = Path.parse_bytes(bytes.concat([root.bytes(), b"/source\xff\xfe"]))?
  let directory = fp"{root}/links"
  let link = Path.parse_bytes(bytes.concat([directory.bytes(), b"/source\xff\xfe"]))?
  source.write("payload")
  directory.mkdir()
  let script = fp"{ctx.core_dir}/ln.xsh"
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let command = process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin, script, "-s", "-t", directory, source], root, {}, b"", out, err)

  assert process.run(command)?.exited_with(0)
  assert link.readlink()? == source
}

test test_ln_missing_destination_has_no_help_hint { |ctx|
  let source = test.temp_file(ctx, contents: b"source")?
  let result = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -s -T $source
  assert result.status.exited_with(1)
  assert result.stderr == f"ln: missing destination file operand after '{source}'\n"
}

test test_ln_backup_suffix_cannot_escape_target_directory { |ctx|
  let root = test.temp_dir(ctx)?
  let source = fp"{root}/source"
  let target = fp"{root}/target"
  source.write("source")
  target.write("old")
  fp"{root}/target_".mkdir()
  run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ln.xsh" -- -S _/../escape -s $source $target
  assert target.readlink()? == source
  assert fp"{target}~".read_text()? == "old"
  assert ! fp"{root}/escape".exists()?
}

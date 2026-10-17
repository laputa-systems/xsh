use lib.gnu as gnu
use support.uu
use support.util_linux as ul

proc parity(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "namei", args, timeout: 5s)?
  let candidate = uu.invoke(s, "namei", args, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

proc parity_paths(s: uu.Scene, args: List[Path]) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference_paths(s, "namei", args, timeout: 5s)?
  let candidate = uu.invoke_paths(s, "namei", args, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

test test_namei_upstream_logic_plain_relative_paths { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "namei1/namei2")
  uu.write(s, "namei1/namei2/a", "a\n")
  uu.write(s, "namei1/namei2/b", "b\n")
  parity(s, ["namei1/namei2/a"])?
  parity(s, ["namei1/namei2/b"])?
  assert uu.read_text(s, "namei1/namei2/a")? == "a\n"
  assert uu.read_text(s, "namei1/namei2/b")? == "b\n"
}

test test_namei_each_output_option_and_long_combination { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree/inner")
  uu.write(s, "tree/inner/file", "file\n")
  let operand = fp"{s.root}/tree/inner/file".display()
  for options in [
    ["-x"], ["--mountpoints"], ["-m"], ["--modes"], ["-o"], ["--owners"],
    ["-l"], ["--long"], ["-n"], ["--nosymlinks"], ["-v"], ["--vertical"],
    ["-Z"], ["--context"], ["-xmo"], ["-l", "-n"], ["-m", "-o", "-v"]
  ] {
    parity(s, options.extend(["--", operand]))?
  }
}

test test_namei_component_walk_follows_relative_and_absolute_symlinks { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree/inner")
  uu.write(s, "tree/inner/file", "file\n")
  uu.mkfifo(s, "tree/pipe")
  uu.symlink(s, "inner/file", "tree/relative")
  uu.symlink(s, fp"{s.root}/tree/inner/file".display(), "tree/absolute")
  parity(s, ["-l", "--", fp"{s.root}/tree/relative".display()])?
  parity(s, ["-l", "--", fp"{s.root}/tree/absolute".display()])?
  parity(s, ["--nosymlinks", "--", fp"{s.root}/tree/relative".display()])?
  parity(s, ["-l", "--", fp"{s.root}/tree/pipe".display()])?
  parity(s, ["-l", "--", "/dev/null"])?
  let quoted = fp"{s.root}/tree/space and quote'"
  quoted.write("quoted path\n")
  parity_paths(s, [p"--", quoted])?
  assert uu.read_text(s, "tree/inner/file")? == "file\n"
  assert uu.at(s, "tree/relative").readlink()? == p"inner/file"
  assert uu.at(s, "tree/absolute").readlink()? == fp"{s.root}/tree/inner/file"
  assert quoted.read_text()? == "quoted path\n"
}

test test_namei_committed_path_fixture_links_and_components { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let tree = fp"{ctx.core_dir}/tests/fixtures/util-linux/paths/tree"
  for name in ["regular", "inner/entry", "relative", "dangling", "loop-a", "root-link"] {
    parity(s, ["-l", "--", fp"{tree}/{name}".display()])?
  }
}

test test_namei_mountpoint_markers_follow_a_real_mount_boundary { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let mount = fs.mount_for(s.root)?
  parity(s, ["--mountpoints", "--", mount.mounted_on.display()])?
  parity(s, ["-x", "--", s.root.display()])?
}

test test_namei_private_same_device_bind_mount_marker { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let fixture = fp"{ctx.core_dir}/tests/fixtures/util-linux/paths/bind"
  uu.mkdir(s, "bind/source")
  uu.mkdir(s, "bind/target")
  uu.write(s, "bind/source/entry", fp"{fixture}/source/entry".read_text()?)
  uu.write(s, "bind/target/sentinel", fp"{fixture}/target/sentinel".read_text()?)
  let source = fp"{s.root}/bind/source"
  let target = fp"{s.root}/bind/target"
  assert fs.stat(source)?.dev == fs.stat(target)?.dev
  var mounted = false
  defer {
    if mounted { run.status umount -- $target }
  }
  let bound = run.capture --text mount --bind $source $target
  assert bound.status.exited_with(0), bound.stderr
  mounted = true
  assert fp"{target}/entry".exists()?
  assert ! fp"{target}/sentinel".exists()?
  parity(s, ["-x", "--", target.display()])?
  parity(s, ["-l", "--", fp"{target}/entry".display()])?
  let unmounted = run.capture --text umount -- $target
  assert unmounted.status.exited_with(0), unmounted.stderr
  mounted = false
  assert fp"{target}/sentinel".exists()?
  assert ! fp"{target}/entry".exists()?
}

test test_namei_dangling_missing_and_looping_components { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tree")
  uu.symlink(s, "missing", "tree/dangling")
  uu.symlink(s, "loop-b", "tree/loop-a")
  uu.symlink(s, "loop-a", "tree/loop-b")
  parity(s, ["tree/dangling"])?
  parity(s, ["tree/missing/child"])?
  parity(s, ["tree/loop-a"])?
  parity(s, ["-n", "tree/loop-a"])?
}

test test_namei_multiple_operands_and_usage_errors { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "first", "first\n")
  uu.mkdir(s, "second")
  parity(s, [])?
  parity(s, ["--not-an-option"])?
  parity(s, ["first", "missing", "second"])?
  parity(s, ["--help"])?
  parity(s, ["-h"])?
}

test test_namei_path_arguments_preserve_non_utf8_components { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let directory = uu.at_bytes(s, b"directory-\xfe")?
  directory.mkdir()
  let name = Path.parse_bytes(bytes.concat([directory.bytes(), b"/leaf-\xff"]))?
  name.write("owned raw path\n")
  parity_paths(s, [p"--", name])?
  parity_paths(s, [p"-m", p"--", name])?
  assert name.read_text()? == "owned raw path\n"
}

test test_namei_permission_denied_from_unprivileged_runner { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  s.root.chmod(0o755)
  uu.mkdir(s, "inaccessible")
  uu.write(s, "inaccessible/secret", "owned path\n")
  let directory = fp"{s.root}/inaccessible"
  defer directory.chmod(0o700)
  directory.chmod(0)
  let operand = fp"{directory}/secret".display()
  for args in [["-l", "--", operand], ["--nosymlinks", "--", operand]] {
    let reference = ul.reference_as_user(s, "namei", args, uid: 10001, timeout: 5s)?
    let candidate = ul.candidate_as_user(s, "namei", args, uid: 10001, timeout: 5s)?
    ul.compare(reference, candidate)
  }
  assert fs.stat(directory)?.mode.bit_and(0o777) == 0
  assert fp"{directory}/secret".read_text()? == "owned path\n"
}

test test_namei_version_provenance { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let banner = gnu.version_text("namei")
  for args in [["--version"], ["-V"]] {
    let reference = ul.reference(s, "namei", args, timeout: 5s)?
    let candidate = uu.invoke(s, "namei", args, timeout: 5s)?
    assert reference.status == 0
    assert reference.stdout == b"namei from util-linux 2.42.4\n"
    assert reference.stderr == b""
    assert candidate.status == reference.status
    assert candidate.stdout == bytes.from_text(f"{banner}\n")
    assert candidate.stderr == b""
  }
}

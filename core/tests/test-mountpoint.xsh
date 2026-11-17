use lib.gnu as gnu
use support.uu
use support.util_linux as ul

proc parity(s: uu.Scene, args: List[Str], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "mountpoint", args, timeout: 5s, vars:)?
  let candidate = uu.invoke(s, "mountpoint", args, vars:, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

proc parity_paths(s: uu.Scene, args: List[Path], vars: Record = {}) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference_paths(s, "mountpoint", args, timeout: 5s, vars:)?
  let candidate = uu.invoke_paths(s, "mountpoint", args, vars:, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

test test_mountpoint_upstream_default_follows_mountpoint_symlink { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "file", "owned file fixture\n")
  uu.mkdir(s, "directory")
  let mount = fs.mount_for(s.root)?
  let link = uu.at(s, "mount-link")
  uu.symlink(s, mount.mounted_on.display(), "mount-link")
  parity(s, ["--", fp"{s.root}/mount-link".display()])?
  assert fs.stat(link, follow_symlinks: false)?.kind == "symlink"
  assert link.readlink()? == mount.mounted_on
}

test test_mountpoint_upstream_nofollow_does_not_follow_mountpoint_symlink { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let mount = fs.mount_for(s.root)?
  uu.symlink(s, mount.mounted_on.display(), "mount-link")
  parity(s, ["--nofollow", "--", fp"{s.root}/mount-link".display()])?
  assert uu.at(s, "mount-link").readlink()? == mount.mounted_on
}

test test_mountpoint_upstream_mutually_exclusive_devno_nofollow { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  parity(s, ["--devno", "--nofollow", "/"])?
}

test test_mountpoint_quiet_for_files_directories_and_dangling_symlinks { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "regular", "regular path\n")
  uu.mkdir(s, "directory")
  uu.symlink(s, "regular", "file-link")
  uu.symlink(s, "missing", "dangling-link")
  for operand in ["regular", "directory", "file-link", "dangling-link", "missing"] {
    parity(s, ["--", f"{s.root}/{operand}"])?
    parity(s, ["--quiet", "--", f"{s.root}/{operand}"])?
    parity(s, ["-q", "--", f"{s.root}/{operand}"])?
  }
  parity(s, ["--nofollow", "--", fp"{s.root}/file-link".display()])?
  parity(s, ["--nofollow", "--", fp"{s.root}/dangling-link".display()])?
  assert uu.read_text(s, "regular")? == "regular path\n"
  assert uu.at(s, "file-link").readlink()? == p"regular"
  assert uu.at(s, "dangling-link").readlink()? == p"missing"
}

test test_mountpoint_filesystem_device_numbers_cover_mount_and_regular_paths { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "regular", "regular path\n")
  let mount = fs.mount_for(s.root)?
  parity(s, ["--fs-devno", "--", mount.mounted_on.display()])?
  parity(s, ["-d", "--", fp"{s.root}/regular".display()])?
  assert uu.read_text(s, "regular")? == "regular path\n"
}

test test_mountpoint_device_number_requires_a_block_device { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  parity(s, ["--devno", "/dev/null"])?
  parity(s, ["-x", "/"])?
  parity(s, ["-q", "-x", "/dev/null"])?
}

test test_mountpoint_devno_reports_owned_block_device { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let device = uu.at(s, "block-device")
  fs.mknod(device, "block", 0o600, major: 8, minor: 0)?
  parity(s, ["--devno", "--", device.display()])?
  parity(s, ["-x", "--", device.display()])?
  assert fs.stat(device)?.kind == "block"
}

test test_mountpoint_show_on_a_real_mount_and_non_mount_path { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let mount = fs.mount_for(s.root)?
  uu.symlink(s, mount.mounted_on.display(), "mount-link")
  parity(s, ["--show", "--", mount.mounted_on.display()])?
  parity(s, ["--show", "--", fp"{s.root}/mount-link".display()])?
  parity(s, ["--show", "--quiet", "--", s.root.display()])?
  parity(s, ["--show", "--fs-devno", "--", mount.mounted_on.display()])?
  parity(s, ["--show", "--nofollow", "--", fp"{s.root}/mount-link".display()])?
  parity(s, ["--show", "--", fp"{mount.mounted_on}//.".display()])?
  parity(s, ["--show", "--", fp"{s.root}//.".display()])?
  parity(s, ["--show", "--", s.root.display()])?
  let proc_mount = fs.mount_for(p"/proc/1")?
  assert proc_mount.mounted_on == p"/proc"
  let proc_show = ul.reference(s, "mountpoint", ["--show", "--", proc_mount.mounted_on.display()], timeout: 5s)?
  assert proc_show.status == 0
  assert proc_show.stdout == b"/proc\n"
  assert proc_show.stderr == b""
  parity(s, ["--show", "--", proc_mount.mounted_on.display()])?
  assert uu.at(s, "mount-link").readlink()? == mount.mounted_on
}

test test_mountpoint_rejects_bad_operand_counts_and_options { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  parity(s, [])?
  parity(s, ["/", "/dev"])?
  parity(s, ["--not-an-option", "/"])?
  parity(s, ["--help"])?
  parity(s, ["-h"])?
}

test test_mountpoint_path_arguments_preserve_non_utf8_names { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let name = uu.at_bytes(s, b"entry-\xff")?
  name.write("owned raw path\n")
  parity_paths(s, [p"--", name])?
  parity_paths(s, [p"--quiet", p"--", name])?
  let spaced = fp"{s.root}/space and quote'"
  spaced.write("quoted path\n")
  parity_paths(s, [p"--", spaced])?
  for locale in ["C", "C.UTF-8"] {
    parity_paths(s, [p"--", name], vars: {LC_ALL: locale})?
  }
  assert name.read_text()? == "owned raw path\n"
  assert spaced.read_text()? == "quoted path\n"
}

test test_mountpoint_option_prefixes_clusters_and_argument_errors { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  for arguments in [
    ["--no", "/"], ["--f", "/"], ["--fs", "/"], ["--s", "/"],
    ["--d", "/"], ["--dev", "/"], ["--he"], ["-qh"], ["-hqz"],
    ["-qz", "/"], ["-z", "/"], ["--=x", "/"],
    ["--quiet=x", "/"], ["--quiet=", "/"], ["--show=1", "/"],
    ["--d=1", "/"], ["--not-an-option=value", "/"],
    ["/", "--quiet"], ["-", "--quiet"], ["--", "--quiet"],
  ] {
    parity(s, arguments)?
  }
}

test test_mountpoint_inaccessible_path_from_unprivileged_runner { |ctx|
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
  for uid in [10001, 10002] {
    for args in [["--", operand], ["--quiet", "--", operand]] {
      let reference = ul.reference_as_user(s, "mountpoint", args, uid:, timeout: 5s)?
      let candidate = ul.candidate_as_user(s, "mountpoint", args, uid:, timeout: 5s)?
      ul.compare(reference, candidate)
    }
  }
  assert fs.stat(directory)?.mode.bit_and(0o777) == 0
  assert fp"{directory}/secret".read_text()? == "owned path\n"
}

test test_mountpoint_private_same_device_bind_mount { |ctx|
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
  parity(s, ["--", target.display()])?
  parity(s, ["--fs-devno", "--", target.display()])?
  parity(s, ["--show", "--", target.display()])?
  let unmounted = run.capture --text umount -- $target
  assert unmounted.status.exited_with(0), unmounted.stderr
  mounted = false
  assert fp"{target}/sentinel".exists()?
  assert ! fp"{target}/entry".exists()?
}

test test_mountpoint_version_provenance { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let banner = gnu.version_text("mountpoint")
  for args in [["--version"], ["-V"]] {
    let reference = ul.reference(s, "mountpoint", args, timeout: 5s)?
    let candidate = uu.invoke(s, "mountpoint", args, timeout: 5s)?
    assert reference.status == 0
    assert reference.stdout == b"mountpoint from util-linux 2.42.4\n"
    assert reference.stderr == b""
    assert candidate.status == reference.status
    assert candidate.stdout == bytes.from_text(f"{banner}\n")
    assert candidate.stderr == b""
  }
}

test test_mountpoint_loop_errno_text { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "loop", "loop")
  for args in [["loop"], ["-d", "loop"], ["--show", "loop"]] {
    let reference = ul.reference(s, "mountpoint", args)?
    assert reference.status == 1 and reference.stdout == b""
    assert reference.stderr == b"mountpoint: loop: Symbolic link loop\n"
    ul.compare(reference, uu.invoke(s, "mountpoint", args, timeout: 5s)?)
  }
}

test test_mountpoint_pipe_descriptor_resolution { |ctx|
  let s = uu.scene(ctx)?
  for args in [["/proc/self/fd/0"], ["-d", "/proc/self/fd/0"]] {
    let reference = ul.reference(s, "mountpoint", args)?
    assert reference.status == 32 and reference.stderr == b""
    assert reference.stdout == b"/proc/self/fd/0 is not a mountpoint\n"
    ul.compare(reference, uu.invoke(s, "mountpoint", args, timeout: 5s)?)
  }
}

test test_mountpoint_show_pipe_descriptor { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  for args in [["--show", "/dev/stdin"], ["--show", "-q", "/dev/stdin"]] {
    let reference = ul.reference(s, "mountpoint", args)?
    assert reference.status == 1 and reference.stdout == b""
    assert reference.stderr == b"mountpoint: --show is not supported on this system\n"
    ul.compare(reference, uu.invoke(s, "mountpoint", args, timeout: 5s)?)
  }
}

test test_mountpoint_show_deleted_working_directory { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  for target in [p"/proc/self/cwd", p"/proc/self/cwd/", p"/proc/self/cwd/..", uu.at(s, "cwd-link")] {
    if target == uu.at(s, "cwd-link") { uu.symlink(s, "/proc/self/cwd", "cwd-link") }
    let args = [p"--show", target]
    let reference_words = ul.reference_argv(s, "mountpoint", args)?
    let candidate_words = uu.argv(s, "mountpoint", args)?
    var outputs: List[Bytes] = []
    var errors: List[Bytes] = []
    for words in [reference_words, candidate_words] {
      let directory = uu.at(s, "deleted-cwd")
      directory.mkdir()
      let out = uu.at(s, "deleted-stdout")
      let err = uu.at(s, "deleted-stderr")
      let wrapper = [p"/bin/sh", p"-c", p"cd \"$1\"; rmdir \"$1\"; shift; exec \"$@\"", p"wrapper", directory].extend(words)
      let result = process.run(process.command_argv(wrapper[0], wrapper, s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s))?
      assert result.exit_code()? == 0
      outputs += [out.read_bytes()?]
      errors += [err.read_bytes()?]
    }
    assert outputs[0] == outputs[1]
    assert errors == [b"", b""]
  }
}

test test_mountpoint_show_memfd { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let args = [p"--show", p"/proc/self/fd/3"]
  let reference_words = ul.reference_argv(s, "mountpoint", args)?
  let candidate_words = uu.argv(s, "mountpoint", args)?
  for words in [reference_words, candidate_words] {
    let out = uu.at(s, "memfd-stdout")
    let err = uu.at(s, "memfd-stderr")
    let wrapper = [p"/usr/bin/python3", p"-c", p"import os,sys; fd=os.memfd_create('test',0); os.dup2(fd,3,inheritable=True); os.execv(sys.argv[1],sys.argv[1:])"].extend(words)
    let result = process.run(process.command_argv(wrapper[0], wrapper, s.root, {LC_ALL: "C"}, b"", out, err, timeout: 5s))?
    assert result.exit_code()? == 1
    assert out.read_bytes()? == b""
    assert err.read_bytes()? == b"mountpoint: --show is not supported on this system\n"
  }
}

test test_mountpoint_invalid_utf8_long_option { |ctx|
  let s = uu.scene(ctx)?
  let result = uu.invoke_paths(s, "mountpoint", [b"--\xff" as Path])?
  assert result.status == 1
  assert result.stdout == b""
  for index in range(result.stderr.len()) { assert result.stderr.byte_at(index) != 0 }
  assert result.stderr == b"mountpoint: unrecognized option: \xff\nTry 'mountpoint --help' for more information.\n"
}

test test_mountpoint_error_with_os_error_in_path { |ctx|
  let s = uu.scene(ctx)?
  parity(s, ["x/nope (os error 5)/z"])?
}

test test_mountpoint_hidden_mount_identity { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let a = uu.at(s, "a")
  a.mkdir()
  var mounted_a = false
  var mounted_b = false
  var mounted_over = false
  defer {
    if mounted_over { run.status umount -- $a }
    if mounted_b { run.status umount -- fp"{a}/b" }
    if mounted_a { run.status umount -- $a }
  }
  assert (run.capture --text mount -t tmpfs inner $a).status.exited_with(0)
  mounted_a = true
  let b = fp"{a}/b"
  b.mkdir()
  assert (run.capture --text mount -t tmpfs innerb $b).status.exited_with(0)
  mounted_b = true
  assert (run.capture --text mount -t tmpfs over $a).status.exited_with(0)
  mounted_over = true
  b.mkdir()
  for args in [[b.display()], ["-q", b.display()], ["-d", b.display()]] { parity(s, args)? }
}

test test_mountpoint_non_utf8_mountinfo_does_not_break_other_queries { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let bad = uu.at_bytes(s, b"bad-\xff")?
  bad.mkdir()
  var mounted = false
  defer { if mounted { run.status umount -- $bad } }
  assert (run.capture --text mount -t tmpfs bad $bad).status.exited_with(0)
  mounted = true
  parity(s, ["/"])?
  assert fs.mount_for(p"/")?.mounted_on == p"/"
  assert fs.mount_for(bad)?.mounted_on == bad
}

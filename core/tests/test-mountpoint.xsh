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

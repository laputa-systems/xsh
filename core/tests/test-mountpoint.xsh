use core.lib.gnu as gnu
use core.tests.support.uu as uu
use core.tests.support.util_linux as ul

proc parity(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference(s, "mountpoint", args, timeout: 5s)?
  let candidate = uu.invoke(s, "mountpoint", args, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

proc parity_paths(s: uu.Scene, args: List[Path]) [fs, process, env, error] -> Result[Unit, Error] {
  let reference = ul.reference_paths(s, "mountpoint", args, timeout: 5s)?
  let candidate = uu.invoke_paths(s, "mountpoint", args, timeout: 5s)?
  ul.compare(reference, candidate)
  Ok()
}

test test_mountpoint_upstream_default_follows_mountpoint_symlink { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "file", "owned file fixture\n")
  uu.mkdir(s, "directory")
  let mount = fs.mount_for(s.root)?
  uu.symlink(s, mount.mounted_on.display(), "mount-link")
  parity(s, ["--", fp"{s.root}/mount-link".display()])?
}

test test_mountpoint_upstream_nofollow_does_not_follow_mountpoint_symlink { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  let mount = fs.mount_for(s.root)?
  uu.symlink(s, mount.mounted_on.display(), "mount-link")
  parity(s, ["--nofollow", "--", fp"{s.root}/mount-link".display()])?
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
}

test test_mountpoint_filesystem_device_numbers_cover_mount_and_regular_paths { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  uu.write(s, "regular", "regular path\n")
  let mount = fs.mount_for(s.root)?
  parity(s, ["--fs-devno", "--", mount.mounted_on.display()])?
  parity(s, ["-d", "--", fp"{s.root}/regular".display()])?
}

test test_mountpoint_device_number_requires_a_block_device { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  let s = uu.scene(ctx)?
  parity(s, ["--devno", "/dev/null"])?
  parity(s, ["-x", "/"])?
  parity(s, ["-q", "-x", "/dev/null"])?
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
  parity(s, ["--show", "--", s.root.display()])?
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
}

test test_mountpoint_private_same_device_bind_mount { |ctx|
  if system.uname()?.sysname != "Linux" { test.skip("requires Linux"); return }
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let fixture = fp"{ctx.core_dir}/tests/fixtures/util-linux/paths/bind"
  let source = fp"{fixture}/source"
  let target = fp"{fixture}/target"
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

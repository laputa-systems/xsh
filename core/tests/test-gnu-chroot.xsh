use support.uu as uu

# origin: gnu chroot/chroot-fail.log
test test_gnu_chroot_chroot_fail_log { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "chroot", [])?, 125)
  uu.fails_with_code(uu.invoke(s, "chroot", ["---", "/", "true"])?, 125)
  let root = uu.invoke(s, "chroot", ["/", "true"])?
  if root.status == 0 {
    uu.fails_with_code(uu.invoke(s, "chroot", ["/", "sh", "-c", "exit 2"])?, 2)
    uu.fails_with_code(uu.invoke(s, "chroot", ["/", "."])?, 126)
    uu.fails_with_code(uu.invoke(s, "chroot", ["/", "no_such"])?, 127)
  } else {
    uu.fails_with_code(root, 125)
  }
  let invalid = uu.invoke(s, "chroot", ["--skip-chdir", ".", "env", "pwd"])?
  uu.fails(invalid)
  uu.stderr_only(invalid, "chroot: option --skip-chdir only permitted if NEWROOT is old '/'\nTry 'chroot --help' for more information.\n")
  if root.status == 0 {
    uu.symlink(s, "/", "isroot")?
    for directory in ["/", "/.", "/../", "isroot"] {
      let changed = uu.invoke(s, "chroot", [directory, "env", "pwd"])?
      uu.succeeds(changed)
      uu.stdout_is(changed, "/\n")
      let kept = uu.invoke(s, "chroot", ["--skip-chdir", directory, "env", "pwd"])?
      uu.succeeds(kept)
      assert kept.stdout != b"/\n"
    }
  }
}

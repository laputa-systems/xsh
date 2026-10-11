##! Transcribed from the MIT-licensed uutils chroot integration tests.
use support.uu as uu

# origin: uutils test_chroot::test_invalid_arg
test test_uu_chroot_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chroot", ["--definitely-invalid"])?
  uu.fails_with_code(r, 125)
}

# origin: uutils test_chroot::test_chroot_skip_chdir_not_root
test test_uu_chroot_chroot_skip_chdir_not_root { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "foobar")?
  let r = uu.invoke(s, "chroot", ["--skip-chdir", "foobar"])?
  uu.fails_with_code(r, 125)
  uu.stderr_contains(r, "chroot: option --skip-chdir only permitted if NEWROOT is old '/'")
}

# origin: uutils test_chroot::test_chroot_skip_chdir_nonexistent
test test_uu_chroot_chroot_skip_chdir_nonexistent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "chroot", ["--skip-chdir", "/nonexistent/sub"])?
  uu.fails_with_code(r, 125)
  uu.stderr_contains(r, "chroot: option --skip-chdir only permitted if NEWROOT is old '/'")
}

# origin: uutils test_chroot::test_preference_of_userspec
test test_uu_chroot_preference_of_userspec { |ctx|
  let s = uu.scene(ctx)?
  let whoami = uu.invoke(s, "whoami", [])?
  let username = whoami.stdout.utf8()?.trim()
  let id = uu.invoke(s, "id", ["-g", "-n"])?
  let group_name = id.stdout.utf8()?.trim()
  uu.mkdir(s, "a")?
  let r = uu.invoke(s, "chroot", ["a", "--user", "fake", "--groups", "ABC,DEF", f"--userspec={username}:{group_name}"])?
  uu.fails_with_code(r, 125)
}

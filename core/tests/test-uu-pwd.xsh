##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_pwd.rs.

use support.uu as uu

# The command starts through the alias while PWD retains that logical spelling.
proc symlinked_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  uu.mkdir(s, "subdir")?
  uu.symlink(s, "subdir", "symdir")?
  Ok({ctx: ctx, root: uu.at(s, "symdir")})
}

# origin: uutils test_pwd::test_invalid_arg
test test_uu_pwd_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pwd", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_pwd::test_default
test test_uu_pwd_default { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pwd", [])?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::test_ignores_non_option_arguments
test test_uu_pwd_ignores_non_option_arguments { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pwd", ["will-fail"])?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
  uu.stderr_is(r, "pwd: ignoring non-option arguments\n")
}

# origin: uutils test_pwd::test_symlinked_logical
test test_uu_pwd_symlinked_logical { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-L"], vars: {PWD: s.root.display()})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.display() + "\n")
}

# origin: uutils test_pwd::test_symlinked_physical
test test_uu_pwd_symlinked_physical { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-P"], vars: {PWD: s.root.display()})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::test_symlinked_default
test test_uu_pwd_symlinked_default { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", [], vars: {PWD: s.root.display()})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::test_symlinked_default_posix
test test_uu_pwd_symlinked_default_posix { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", [], vars: {PWD: s.root.display(), POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.display() + "\n")
}

# origin: uutils test_pwd::test_symlinked_default_posix_l
test test_uu_pwd_symlinked_default_posix_l { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-L"], vars: {PWD: s.root.display(), POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.display() + "\n")
}

# origin: uutils test_pwd::test_symlinked_default_posix_p
test test_uu_pwd_symlinked_default_posix_p { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-P"], vars: {PWD: s.root.display(), POSIXLY_CORRECT: "1"})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::untrustworthy_pwd_var::test_nonexistent_logical
test test_uu_pwd_untrustworthy_pwd_var_nonexistent_logical { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-L"], vars: {PWD: "/somefakedir"})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::untrustworthy_pwd_var::test_wrong_logical
test test_uu_pwd_untrustworthy_pwd_var_wrong_logical { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-L"], vars: {PWD: s.root.parent().display()})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::untrustworthy_pwd_var::test_redundant_logical
test test_uu_pwd_untrustworthy_pwd_var_redundant_logical { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-L"], vars: {PWD: s.root.display() + "/."})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

# origin: uutils test_pwd::untrustworthy_pwd_var::test_relative_logical
test test_uu_pwd_untrustworthy_pwd_var_relative_logical { |ctx|
  let s = symlinked_scene(ctx)?
  let r = uu.invoke(s, "pwd", ["-L"], vars: {PWD: "."})?
  uu.succeeds(r)
  uu.stdout_is(r, s.root.resolve()?.display() + "\n")
}

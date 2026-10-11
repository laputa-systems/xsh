##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_users.rs.

use support.uu as uu

# origin: uutils test_users::test_invalid_arg
test test_uu_users_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "users", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_users::test_users_no_arg
test test_uu_users_users_no_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "users", [])?
  uu.succeeds(r)
}

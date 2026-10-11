##! Transcribed from the MIT-licensed uutils logname integration tests.

use support.uu as uu

# origin: uutils test_logname::test_invalid_arg
test test_uu_logname_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "logname", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

##! Transcribed zero-increment tests from the uutils coreutils suite.

use support.uu as uu

# origin: uutils test_seq::test_zero_step
test test_uu_seq_zero_step { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["10", "0", "32"])?
  uu.fails(r)
}

# origin: uutils test_seq::test_zero_step_floats
test test_uu_seq_zero_step_floats { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "seq", ["10.0", "0", "32"])?
  uu.fails(r)
}

##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_tr.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_tr::uppercase_conversion_works_1
test test_uu_tr_uppercase_conversion_works_1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
}

# origin: uutils test_tr::uppercase_conversion_works_2
test test_uu_tr_uppercase_conversion_works_2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a-z", "A-Z"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
}

# origin: uutils test_tr::uppercase_conversion_works_3
test test_uu_tr_uppercase_conversion_works_3 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["[:lower:]", "[:upper:]"], stdin: b"abcdefghijklmnopqrstuvwxyz")?
  uu.succeeds(r)
  uu.stdout_is(r, "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
}

# origin: uutils test_tr::translate_complement_set_in_order
test test_uu_tr_translate_complement_set_in_order { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-c", "@-~", " -^"], stdin: b"01234")?
  uu.succeeds(r)
  uu.stdout_is(r, "PQRST")
}

# origin: uutils test_tr::tr_truncate_set1_longer_than_set2
test test_uu_tr_tr_truncate_set1_longer_than_set2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["-t", "mnop", "pq"], stdin: b"mnopq")?
  uu.succeeds(r)
  uu.stdout_is(r, "pqopq")
}

# origin: uutils test_tr::tr_translate_with_null_byte
test test_uu_tr_tr_translate_with_null_byte { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tr", ["a", ""], stdin: b"")?
  uu.fails(r)
  uu.stderr_is(r, "tr: when not truncating set1, string2 must be non-empty\n")
}

test test_locale_c_metadata {
  for name in ["C", "POSIX"] {
    let numeric = locale.numeric_info(name)?
    assert numeric.decimal_point == b"."
    assert numeric.thousands_separator == b""
    let time = locale.time_info(name)?
    assert time.abbreviated_months == [b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec"]
  }
}

test test_locale_rejects_invalid_names {
  test.error_kind(locale.numeric_info(""), "locale-name")
  test.error_kind(locale.time_info("C\0suffix"), "locale-name")
  test.error_kind(locale.numeric_info("xsh_nonexistent_locale.UTF-8"), "locale-unavailable")
  test.error_kind(locale.time_info("xsh_nonexistent_locale.UTF-8"), "locale-unavailable")
  assert (locale.numeric_info("C")?).decimal_point == b"."
}

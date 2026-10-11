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

test test_locale_french_bytes_and_encoding_aliases {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("Linux supplies the committed GNU libc locale metadata")
    return
  }
  let french = locale.numeric_info("fr_FR.UTF-8")?
  assert french.decimal_point == b","
  assert french.thousands_separator == b"\xe2\x80\xaf"
  assert (locale.time_info("fr_FR.utf-8")?).abbreviated_months[1] == b"f\xc3\xa9vr."
  let iso = locale.numeric_info("fr_FR.ISO-8859-1")?
  assert iso.thousands_separator == b"\xa0"
  assert (locale.time_info("fr_FR")?).abbreviated_months[1] == b"f\xe9vr."
  assert (locale.numeric_info("C.UTF-8")?).decimal_point == b"."
  assert (locale.numeric_info("C")?).thousands_separator == b""
}

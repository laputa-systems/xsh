test test_numeric_long_double_prefix_and_range {
  let parsed = numeric.parse_long_double(" \t-0x1.8p+2tail")?
  assert parsed.consumed == 11
  assert !parsed.range_error
  assert (numeric.parse_long_double("invalid")?).consumed == 0
  assert (numeric.parse_long_double("1\0junk")?).consumed == 1
  assert (numeric.parse_long_double("1e99999")?).range_error
  assert (numeric.parse_long_double("1e-99999")?).range_error
}

test test_numeric_long_double_total_order {
  let inputs = ["invalid", "nan", "-inf", "-2", "-0.1", "-0", "0.1", "2", "inf"]
  var previous = ""
  for input in inputs {
    let key = (numeric.parse_long_double(input)?).order_key
    assert previous < key
    previous = key
  }
  assert (numeric.parse_long_double("-0")?).order_key == (numeric.parse_long_double("0")?).order_key
  assert (numeric.parse_long_double("nan(foo)")?).order_key == (numeric.parse_long_double("-nan")?).order_key
  assert (numeric.parse_long_double("1")?).order_key == (numeric.parse_long_double("0x1p0")?).order_key
  if numeric.long_double_precision() > 53 {
    assert (numeric.parse_long_double("1.000000000000000001")?).order_key > (numeric.parse_long_double("1")?).order_key
  }
}

test test_numeric_long_double_formatting {
  assert (numeric.format_long_double("1.25tail", "f", precision: 2)?).text == "1.25"
  assert (numeric.format_long_double("1.25tail", "f", precision: 2)?).consumed == 4
  assert (numeric.format_long_double("-0", "f", precision: 1)?).text == "-0.0"
  assert (numeric.format_long_double("1", "g", alternate: true)?).text == "1.00000"
  let hex = (numeric.format_long_double("0.875", "a")?).text
  assert (numeric.parse_long_double(hex)?).order_key == (numeric.parse_long_double("0.875")?).order_key
  if numeric.long_double_precision() == 64 {
    assert (numeric.format_long_double("0.1", "f", precision: 30)?).text == "0.100000000000000000001355252716"
  }
  if numeric.long_double_precision() == 64 {
    assert (numeric.format_long_double("0.9999995", "f")?).text == "0.999999"
  }
  test.error_kind(numeric.format_long_double("1", "%f"), "numeric-long-double")
  test.error_kind(numeric.format_long_double("1", "f", precision: -1), "numeric-long-double")
  test.error_kind(numeric.format_long_double("1", "f", precision: 2147483648), "numeric-long-double")
}

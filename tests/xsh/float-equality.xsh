pure equals_zero(value: Float) -> Bool {
  value == 0.0
}

pure matches_zero_pattern(value: Float) -> Bool {
  match value {
    0.0 => true
    _ => false
  }
}

const constant_positive_zero = 0.0
const constant_negative_zero = -0.0
const constant_same_nan = 0.0 / 0.0
const constant_signed_zero_equal = constant_positive_zero == constant_negative_zero
const constant_same_nan_unequal = constant_same_nan != constant_same_nan
const constant_nested_zero_equal = [[constant_positive_zero]] == [[constant_negative_zero]]
const constant_nested_nan_unequal = [[constant_same_nan]] != [[constant_same_nan]]

test test_float_runtime_equality_uses_ieee_values {
  let positive_zero = 0.0
  let negative_zero = -0.0
  let nan = 0.0 / 0.0

  assert positive_zero == negative_zero
  assert nan != nan
  assert equals_zero(negative_zero)
  assert matches_zero_pattern(negative_zero)
  assert [positive_zero] == [negative_zero]
  assert [nan] != [nan]
  assert negative_zero in [positive_zero]
  assert nan not in [nan]
  test.eq(positive_zero, negative_zero)?
  test.ne(nan, nan)?
}

test test_float_constant_equality_uses_ieee_values {
  assert constant_signed_zero_equal
  assert constant_same_nan_unequal
  assert constant_nested_zero_equal
  assert constant_nested_nan_unequal
}

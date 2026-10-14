const constant_positive_zero = 0.0
const constant_negative_zero = -0.0
const constant_same_nan = 0.0 / 0.0
const constant_signed_zero_equal = constant_positive_zero == constant_negative_zero
const constant_same_nan_unequal = constant_same_nan != constant_same_nan
const constant_nested_zero_equal = [[constant_positive_zero]] == [[constant_negative_zero]]
const constant_nested_nan_unequal = [[constant_same_nan]] != [[constant_same_nan]]

pure equals_zero(value: Float) -> Bool {
  value == 0.0
}

pure matches_zero_pattern(value: Float) -> Bool {
  match value {
    0.0 => true
    else => false
  }
}

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
  let _ = test.eq(positive_zero, negative_zero)?
  let _ = test.ne(nan, nan)?
}

test test_float_constant_equality_uses_ieee_values {
  assert constant_signed_zero_equal
  assert constant_same_nan_unequal
  assert constant_nested_zero_equal
  assert constant_nested_nan_unequal
}

test test_keyed_stages_use_ieee_float_equality {
  let positive_zero = 0.0
  let negative_zero = -0.0
  let zero_values = [positive_zero, negative_zero]

  let unique_zeros = zero_values |> unique-by .
  assert unique_zeros.len() == 1

  let zero_groups = zero_values |> group-by .
  assert zero_groups.len() == 1
  assert zero_groups[0].items.len() == 2
  assert zero_groups[0].key == positive_zero

  let nan = 0.0 / 0.0
  let nan_values = [nan, nan]

  let unique_nans = nan_values |> unique-by .
  assert unique_nans.len() == 2

  let nan_groups = nan_values |> group-by .
  assert nan_groups.len() == 2
  assert nan_groups[0].items.len() == 1
  assert nan_groups[1].items.len() == 1
  assert nan_groups[0].key != nan_groups[0].key
}

test test_keyed_stages_compare_recursive_float_keys_semantically {
  let positive_zero = 0.0
  let negative_zero = -0.0
  let zero_keys = [[positive_zero], [negative_zero]]

  let unique_zero_keys = zero_keys |> unique-by .
  assert unique_zero_keys.len() == 1

  let zero_key_groups = zero_keys |> group-by .
  assert zero_key_groups.len() == 1
  assert zero_key_groups[0].items.len() == 2
  assert zero_key_groups[0].key == [positive_zero]

  let nan = 0.0 / 0.0
  let nan_keys = [[nan], [nan]]

  let unique_nan_keys = nan_keys |> unique-by .
  assert unique_nan_keys.len() == 2

  let nan_key_groups = nan_keys |> group-by .
  assert nan_key_groups.len() == 2
  assert nan_key_groups[0].key[0] != nan_key_groups[0].key[0]
  assert nan_key_groups[1].key[0] != nan_key_groups[1].key[0]
}

export pure invalid_option(option: Str) -> Str {
  return f"invalid option -- ${option}"
}

export pure missing_option_value(option: Str) -> Str {
  return f"missing value for option -- ${option}"
}

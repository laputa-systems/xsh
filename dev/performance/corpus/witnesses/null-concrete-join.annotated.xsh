pure nullable_join(flag: Bool) -> Int? {
  return 7 when flag
  return null
}

print ${nullable_join(true) ?? -1}
print ${nullable_join(false) ?? -1}

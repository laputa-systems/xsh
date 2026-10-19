pure nullable_join(flag: Bool) -> Int? {
  if flag {
    return 7
  }
  return null
}

print ${nullable_join(true) ?? -1}
print ${nullable_join(false) ?? -1}

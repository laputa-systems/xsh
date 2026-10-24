pure recursive_parameter(count: Int) -> Int {
  if count <= 0 {
    return 0
  }
  return recursive_parameter(count - 1) + 1
}

print ${recursive_parameter(4)}

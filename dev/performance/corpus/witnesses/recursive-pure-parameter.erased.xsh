pure recursive_parameter(count) -> Int {
  return 0 when count <= 0
  return recursive_parameter(count - 1) + 1
}

print ${recursive_parameter(4)}

pure recursive_return(count: Int) -> Int {
  return 0 when count <= 0
  return recursive_return(count - 1) + 1
}

print ${recursive_return(4)}

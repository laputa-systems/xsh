pure recursive_return(count: Int) {
  if count <= 0 {
    return 0
  }
  return recursive_return(count - 1) + 1
}

print ${recursive_return(4)}

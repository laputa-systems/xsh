proc main() [env, error, io] {
  let size = env.int("XSH_RANGE_SIZE", 16)?
  var last = 0
  for item in range(size) {
    last = item
    if item == 3 { break }
  }
  print f"{last}"
}

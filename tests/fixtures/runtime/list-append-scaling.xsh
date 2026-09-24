# The harness varies the size to guard repeated List.push accumulation traffic.
proc main() [env, error, io] {
  let size = env.int("XSH_LIST_APPEND_SIZE", 128)?
  var items: List[Int] = []
  var index = 0
  while index < size {
    items = items.push(index)
    index += 1
  }
  print f"${items.len()} ${items.get(size - 1)?}"
}

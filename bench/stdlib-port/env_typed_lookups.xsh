proc main() [io, env, error] {
  var sink = 0
  var round = 0
  while round < 500 {
    let fallback = env.get_or("XSH_BENCH_MISSING", "fallback")
    sink = sink + (fallback ?? "?").byte_len()
    match env.bool("XSH_BENCH_FLAG") {
      Ok(flag) => {
        sink = sink + (if flag { 1 } else { 0 })
      }
      Err(_) => {
        sink = sink + 1
      }
    }
    match env.int("XSH_BENCH_COUNT") {
      Ok(count) => {
        sink = sink + count
      }
      Err(_) => {
        sink = sink + 2
      }
    }
    round = round + 1
  }
  print f"${sink}"
}

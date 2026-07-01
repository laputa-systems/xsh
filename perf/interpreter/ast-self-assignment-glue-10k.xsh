proc main() [fs, error] {
  var rows: List[Str] = []
  var counts: Map[Int] = {}
  var i = 0

  while i < 10000 {
    let key = if i % 3 == 0 { "alpha" } else if i % 3 == 1 { "beta" } else { "gamma" }
    rows = rows.push(key)
    counts[key] = counts.get(key, 0) + 1
    i += 1
  }

  print (rows.len() + counts.get("alpha", 0) * 3 + counts.get("beta", 0) * 5 + counts.get("gamma", 0) * 7) ) % 256
}

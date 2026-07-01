pure scan_line(line: Str) -> Int {
  let n = line.byte_len()
  var index = 0
  var score = 0
  var in_string = false
  var delim = -1

  while index < n {
    let ch = line.byte_at(index)
    let next = line.byte_at(index + 1)

    if in_string {
      if ch == delim {
        in_string = false
      } else {
        score += ch % 7
      }
    } else if ch == 47 and next == 47 {
      return score
    } else if ch == 34 or ch == 39 {
      in_string = true
      delim = ch
    } else if ch != 32 and ch != 9 {
      score += 1
    }

    index += 1
  }

  return score
}

pure scan_many(line: Str, limit: Int) -> Int {
  var total = 0
  var i = 0

  while i < limit {
    total += scan_line(line)
    i += 1
  }

  return total
}

let line = "let path = \"src/main.rs\"; let marker = 'x'; // trailing comment"
let total = scan_many(line, 10000)

print $total % 256

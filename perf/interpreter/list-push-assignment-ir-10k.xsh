pure collect_lines(text: Str) -> Int {
  var lines = [line.trim() for line in text.lines()]
  return lines.join(",").byte_len()
}

let sample = """
alpha
 beta
  gamma
delta
 epsilon
  zeta
eta
 theta
"""

var i = 0
var total = 0

while i < 10000 {
  total += collect_lines(sample)
  i += 1
}

print $total % 256

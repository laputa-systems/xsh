type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}

type Scan = {stats: Stats, deep: Stats}

pure count_hash_unindented(text: Str) -> Scan {
  var blanks = 0
  var comments = 0

  for line in text.lines() {
    if line == "" {
      blanks += 1
    } else if line.starts_with("#") {
      comments += 1
    }
  }

  let code = text.count_lines() - blanks - comments
  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

let sample = """# header
msgid "alpha"
msgstr "beta"

# translator note
msgid "gamma"
msgstr "delta"
"""

var i = 0
var total = 0

while i < 5000 {
  let scan = count_hash_unindented(sample)
  total += scan.deep.blanks + scan.deep.code + scan.deep.comments
  i += 1
}

print $total % 256

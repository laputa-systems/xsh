type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}

type Scan = {stats: Stats, deep: Stats}

pure count_code_text(text: Str) -> Scan {
  var blanks = 0
  var code = 0

  for line in text.lines() {
    if line.trim() == "" {
      blanks += 1
    } else {
      code += 1
    }
  }

  let stats = {blanks, code, comments: 0, blobs: map.empty()}
  return {stats, deep: stats}
}

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

pure count_hash_language(text: Str) -> Scan {
  if ! text.contains("#") {
    return count_code_text(text)
  }

  if ! text.starts_with(" ") and ! text.starts_with("\t") and ! text.contains("""
 """) and ! text.contains("""
	""") {
    return count_hash_unindented(text)
  }

  var blanks = 0
  var comments = 0

  for line in text.lines() {
    if line.trim() == "" {
      blanks += 1
    } else if line.trim().starts_with("#") {
      comments += 1
    }
  }

  let code = text.count_lines() - blanks - comments
  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

let sample = """def handler(event):
    value = event["id"]
    if value:
        return value

    # fallback path
    return "missing"

def other():
    return 42
"""

var i = 0
var total = 0

while i < 5000 {
  let scan = count_hash_language(sample)
  total += scan.deep.blanks + scan.deep.code + scan.deep.comments
  i += 1
}

print $total % 256

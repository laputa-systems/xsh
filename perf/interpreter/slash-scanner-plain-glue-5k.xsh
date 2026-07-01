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

pure count_slash_plain(text: Str) -> Scan {
  if ! text.contains("/") {
    return count_code_text(text)
  }

  if ! text.contains("//") and ! text.contains("/*") {
    return count_code_text(text)
  }

  var blanks = 0
  var code = 0
  var comments = 0
  var block_depth = 0

  for line in text.lines() {
    if block_depth == 0 and ! line.contains("/") {
      if line.trim() == "" {
        blanks += 1
      } else {
        code += 1
      }
    } else if block_depth == 0 and ! line.contains("//") and ! line.contains("/*") {
      code += 1
    } else {
      let trimmed = line.trim()

      if trimmed == "" and block_depth == 0 {
        blanks += 1
      } else if block_depth == 0 and trimmed.starts_with("//") {
        comments += 1
      } else if block_depth == 0 and ! trimmed.starts_with("/") and (! line.contains("/*") or line.contains("*/")) {
        code += 1
      } else if block_depth == 0 and trimmed.starts_with("/*") and ! line.contains("*/") {
        comments += 1
        block_depth = 1
      } else if block_depth > 0 and trimmed == "" {
        blanks += 1
      } else if block_depth > 0 and ! line.contains("*/") {
        comments += 1
      } else if block_depth > 0 and trimmed.ends_with("*/") {
        comments += 1
        block_depth -= 1
      } else {
        let line_len = line.byte_len()
        var index = 0
        var code_seen = false
        var comment_seen = false
        var in_string = false
        var string_delim = -1
        var escaped = false

        while index < line_len {
          let ch = line.byte_at(index)
          let next = line.byte_at(index + 1)

          if block_depth > 0 {
            comment_seen = true

            if ch == 42 and next == 47 {
              block_depth -= 1
              index += 2
            } else {
              index += 1
            }
          } else if in_string {
            code_seen = true

            if escaped {
              escaped = false
            } else if ch == 92 {
              escaped = true
            } else if ch == string_delim {
              in_string = false
            }

            index += 1
          } else if ch == 34 or ch == 39 or ch == 96 {
            code_seen = true
            in_string = true
            string_delim = ch
            index += 1
          } else if ch == 47 and next == 47 {
            comment_seen = true
            index = line_len
          } else if ch == 47 and next == 42 {
            comment_seen = true
            block_depth = 1
            index += 2
          } else {
            if ch != 32 and ch != 9 {
              code_seen = true
            }

            index += 1
          }
        }

        if code_seen {
          code += 1
        } else if comment_seen {
          comments += 1
        } else {
          blanks += 1
        }
      }
    }
  }

  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

let sample = """import path from "node:path"
const url = "https://example.test/path"
const template = `value/\${name}`
// trailing note
let code = path.join("src", "index.ts")

/*
block note
*/
export const answer = 42
"""

var i = 0
var total = 0

while i < 5000 {
  let scan = count_slash_plain(sample)
  total += scan.deep.blanks + scan.deep.code + scan.deep.comments
  i += 1
}

print $total % 256

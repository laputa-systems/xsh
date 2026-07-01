type Stats = {blanks: Int, code: Int, comments: Int, docs: Int}

pure count_code_text(text: Str) -> Stats {
  var blanks = 0
  var code = 0

  for line in text.lines() {
    if line.trim() == "" {
      blanks += 1
    } else {
      code += 1
    }
  }

  return {blanks, code, comments: 0, docs: 0}
}

pure count_slash_language(text: Str, nested: Bool, collect_doc_markdown: Bool) -> Stats {
  if ! text.contains("/") {
    return count_code_text(text)
  }

  if ! text.contains("//") and ! text.contains("/*") {
    return count_code_text(text)
  }

  var blanks = 0
  var code = 0
  var comments = 0
  var doc_lines: List[Str] = []
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

      if collect_doc_markdown and block_depth == 0 and (trimmed.starts_with("///") or trimmed.starts_with("//!")) {
        doc_lines = doc_lines.push(trimmed.byte_slice(3))
      } else if trimmed == "" and block_depth == 0 {
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
      } else if block_depth > 0 and ! line.contains("*/") and (! nested or ! line.contains("/*")) {
        comments += 1
      } else if block_depth > 0 and trimmed.ends_with("*/") and (! nested or ! line.contains("/*")) {
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

            if ch == 47 and next == 42 and nested {
              block_depth += 1
              index += 2
            } else if ch == 42 and next == 47 {
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

  return {blanks, code, comments, docs: doc_lines.len()}
}

let sample = """pub fn build_message(input: &str) -> String {
    let alpha = input.trim()
    let beta = alpha.to_string()
    let gamma = beta.len()

    /// Build the visible message.
    /// Keep markdown collection on this path.
    let url = "https://example.test/path"
    /*
      outer note
      /* nested note */
    */
    let delta = gamma + 1
    let epsilon = delta + 2
    // trailing note
    format!("{} {}", url, epsilon)
}
"""

var i = 0
var total = 0

while i < 5000 {
  let scan = count_slash_language(sample, true, true)
  total += scan.blanks + scan.code + scan.comments + scan.docs
  i += 1
}

print $total % 256

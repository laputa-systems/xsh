pure scan_slash_text(text: Bytes) -> Bool {
  var block_depth = 0
  var any_code = false
  var any_comment = false
  for line in text.lines() {
    let line_len = line.len()
    var index = 0
    var code_seen = false
    var comment_seen = false
    var in_string = false
    var string_delim = -1
    var escaped = false
    while index < line_len {
      let ch = line.byte_at(index) ?? -1
      let next = line.byte_at(index + 1) ?? -1
      if block_depth > 0 {
        comment_seen = true
        any_code = true
        if ch == 47 and next == 42 {
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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code
}


test byte_scan {
  let _ = scan_slash_text(b"/*x*/")
}

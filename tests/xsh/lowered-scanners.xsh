pure count_lines(text: Bytes) -> Int {
  var blanks = 0
  var comments = 0
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == b"" {
      blanks += 1
    } else if trimmed.starts_with(b"//") {
      comments += 1
    }
  }
  text.count_lines() - blanks - comments
}

pure count_text_lines(text: Str) -> Int {
  var blanks = 0
  var comments = 0
  for line in text.lines() {
    let trimmed = line.trim()
    if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("//") {
      comments += 1
    }
  }
  text.count_lines() - blanks - comments
}

pure scan_slash_nested(text: Bytes) -> Bool {
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
  any_code and any_comment
}

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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code and any_comment
}

test test_scan_statement_cohort_runs_on_forced_frames {
  assert count_lines(b"// comment\r\n\r\nfn main() {}") == 1
  assert count_text_lines("// note\r\n\r\nπ + 1") == 1
  assert scan_slash_nested(b"/* open\r\nstill comment\r\n*/\r\n\"//\" // comment")
  assert scan_slash_text(b"/* open\r\nstill comment\r\n*/\r\n\"//\" // comment")
}

pure scan_extra_block(text: Bytes) -> Bool {
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


test test_scan_extra_block {
  assert scan_extra_block(b"/*x*/") == true
}

pure scan_extra_line_comment(text: Bytes) -> Bool {
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
        any_code = true
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


test test_scan_extra_line_comment {
  assert scan_extra_line_comment(b"//") == true
}

pure scan_step_three(text: Bytes) -> Bool {
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
        index += 3
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


test test_scan_step_three {
  assert scan_step_three(b"/**/x") == false
}

pure scan_else_effect(text: Bytes) -> Bool {
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
        any_code = true
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


test test_scan_else_effect {
  assert scan_else_effect(b" ") == true
}

pure scan_nested_depth(text: Bytes) -> Bool {
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


test test_scan_nested_depth {
  assert scan_nested_depth(b"/* /* */ x */") == false
}

pure scan_index_after(text: Bytes) -> Bool {
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
    any_code = index == line_len
    any_comment = any_comment or comment_seen
  }
  any_code
}


test test_scan_index_after {
  assert scan_index_after(b"x") == true
}

pure scan_initial_index(text: Bytes) -> Bool {
  var block_depth = 0
  var any_code = false
  var any_comment = false
  for line in text.lines() {
    let line_len = line.len()
    var index = 1
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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code
}


test test_scan_initial_index {
  assert scan_initial_index(b"x ") == false
}

pure scan_initial_flag(text: Bytes) -> Bool {
  var block_depth = 0
  var any_code = false
  var any_comment = false
  for line in text.lines() {
    let line_len = line.len()
    var index = 0
    var code_seen = true
    var comment_seen = false
    var in_string = false
    var string_delim = -1
    var escaped = false
    while index < line_len {
      let ch = line.byte_at(index) ?? -1
      let next = line.byte_at(index + 1) ?? -1
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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code
}


test test_scan_initial_flag {
  assert scan_initial_flag(b" ") == true
}

pure scan_line_limit(text: Bytes) -> Bool {
  var block_depth = 0
  var any_code = false
  var any_comment = false
  for line in text.lines() {
    let line_len = 0
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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code
}


test test_scan_line_limit {
  assert scan_line_limit(b"x") == false
}

pure scan_depth_update(text: Bytes) -> Bool {
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
        block_depth = 2
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


test test_scan_depth_update {
  assert scan_depth_update(b"/**/x") == false
}

pure scan_extra_quote(text: Bytes) -> Bool {
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
      } else if ch == 34 or ch == 39 or ch == 96 or ch > 119 {
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
  any_code and any_comment
}


test test_scan_extra_quote {
  assert scan_extra_quote(b"x//") == false
}

pure scan_extra_block_canonical(text: Bytes) -> Bool {
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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code
}



test test_scan_extra_block_canonical {
  assert scan_extra_block_canonical(b"/*x*/") == true
}

test test_scan_str_lines { |ctx|
  test.expect(
    ctx,
    r"""pure scan_str_lines(text: Str) -> Bool {
  var block_depth = 0
  var any_code = false
  var any_comment = false
  for line in text.lines() {
    let line_len = line.byte_len()
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
    any_code = any_code or code_seen
    any_comment = any_comment or comment_seen
  }
  any_code and any_comment
}

print scan_str_lines("/* open */ \"x\"")
""",
    status: 0,
    stdout: ["true"],
  )?
}

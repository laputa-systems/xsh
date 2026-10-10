##! Ed script application for the patch applet. GNU patch feeds an ed script
##! to ed; here the commands diff -e writes run against the file's lines
##! directly.

use patch_parse

const ED_ADDRESS_RE = rx"^(\.|\$|\d+)?(?:,(\.|\$|\d+))?\s*([a-zA-Z=]?)(.*)$"

## Result of running an ed script: the new content, or the text of the first
## command that could not be carried out.
export type EdResult = {output: List[Bytes], failed: Str, next: Int}

pure ed_address(text: Str, current: Int, last: Int) -> Int {
  if text == "." { return current }
  if text == "$" { return last }
  text.parse_int() ?? -1
}

pure with_newline(text: Bytes) -> Bytes {
  if text.ends_with(b"\n") { text } else { bytes.concat([text, b"\n"]) }
}

## Apply an ed script that starts at `lines[at]` to `input` the way `ed -`
## does: commands run in order against the whole buffer, and text for `a`,
## `i`, and `c` ends at a line holding a single dot.
export pure run_ed(lines: List[Bytes], at: Int, input: List[Bytes]) -> EdResult {
  var buffer = input
  var current = buffer.len()
  var i = at
  while i < lines.len() {
    let text = patch_parse.body_text(lines[i])
    # GNU patch hands the editor only the commands that start with a digit
    # (and the text that follows them); anything else is dropped.
    if !rx"^[0-9]".matches(text) {
      i += 1
      continue
    }
    let c = ED_ADDRESS_RE.captures(text)
    if c.is_empty() { return {output: buffer, failed: text, next: i} }
    let command = c[3]
    let size = buffer.len()
    var first = if c[1] == "" { current } else { ed_address(c[1], current, size) }
    var last = if c[2] == "" { first } else { ed_address(c[2], current, size) }
    if c[1] == "" and c[2] == "" and command != "" { first = current; last = current }
    i += 1
    if command == "a" or command == "i" or command == "c" {
      var added: List[Bytes] = []
      var closed = false
      while i < lines.len() {
        let body = patch_parse.body_text(lines[i])
        i += 1
        if body == "." { closed = true; break }
        added += [with_newline(lines[i - 1])]
      }
      if !closed { return {output: buffer, failed: text, next: i} }
      if command == "a" {
        if first < 0 or first > size { return {output: buffer, failed: text, next: i} }
        buffer = bytes_list_splice(buffer, first, first, added)
        current = first + added.len()
      } else if command == "i" {
        let at_line = if first < 1 { 1 } else { first }
        if at_line > size + 1 { return {output: buffer, failed: text, next: i} }
        buffer = bytes_list_splice(buffer, at_line - 1, at_line - 1, added)
        current = at_line - 1 + added.len()
      } else {
        if first < 1 or last > size or last < first { return {output: buffer, failed: text, next: i} }
        buffer = bytes_list_splice(buffer, first - 1, last, added)
        current = first - 1 + added.len()
      }
    } else if command == "d" {
      if first < 1 or last > size or last < first { return {output: buffer, failed: text, next: i} }
      buffer = bytes_list_splice(buffer, first - 1, last, [])
      current = if first - 1 < buffer.len() { first } else { buffer.len() }
    } else if command == "s" {
      let parts = c[4].split(c[4].byte_slice(0, length: 1))
      if parts.len() < 3 or first < 1 or last > size { return {output: buffer, failed: text, next: i} }
      let pattern = regex.compile(parts[1])
      let compiled = match pattern {
        Ok(value) => value
        Err(_) => { return {output: buffer, failed: text, next: i} }
      }
      let global = parts.len() > 3 and parts[3] == "g"
      for index in range(first - 1, last) {
        let line = patch_parse.body_text(buffer[index])
        let found = compiled.find(line)
        var replaced = line
        if !found.is_empty() {
          if global { replaced = compiled.replace(line, with: parts[2]) } else {
            replaced = line.byte_slice(0, found[0].start) + parts[2] + line.byte_slice(found[0].end)
          }
        }
        buffer[index] = bytes.from_text(replaced + "\n")
      }
      current = last
    } else if command == "w" or command == "q" or command == "p" or command == "" {
      # Writing and quitting are implicit; nothing else in an ed script
      # changes the buffer.
    } else {
      return {output: buffer, failed: text, next: i}
    }
  }
  {output: buffer, failed: "", next: i}
}

pure bytes_list_splice(list: List[Bytes], from: Int, to: Int, middle: List[Bytes]) -> List[Bytes] {
  var output: List[Bytes] = []
  for index in range(from) { output += [list[index]] }
  output += middle
  for index in range(to, list.len()) { output += [list[index]] }
  output
}

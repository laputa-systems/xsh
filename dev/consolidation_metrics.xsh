##! Production Rust source inventories and consolidation ratchets.

## A scanner or ratchet failure with source evidence.
export error MetricError = Invalid(path: Str, line: Int, detail: Str) | Rise(name: Str, before: Int, after: Int) | Contract(detail: Str)

type Token = {text: Str, start: Int, end: Int, line: Int, last_line: Int, literal: Bool}
type Lexed = {tokens: Map[Int, Token], mates: Map[Int], size: Int, lines: Int}
type Span = {start: Int, end: Int, line: Int, last_line: Int}
type Attribute = {start: Int, end: Int, inner: Bool, scope: Int, condition_active: Bool}
type RustModule = {name: Str, directory: Str, explicit_path: Str?, excluded: Bool, dead_code: Bool, line: Int}
type CfgValue = {value: Bool, next: Int}
type CfgPredicate = {start: Int, end: Int}
type InlineModule = {end: Int, directory: Str}
type ModuleEdge = {owner: Str, target: Str, excluded: Bool, dead_code: Bool}

## One physical Rust file, with exact excluded spans and retained line numbers.
export type SourceCount = {path: Str, physical: Int, production: Int, retained_lines: List[Int], excluded: List[Span]}
## A structural occurrence; each contributes one unit unless its detail states a line scope.
export type Evidence = {path: Str, line: Int, detail: Str}
## A structural metric and the occurrences that determine its value.
export type Metric = {name: Str, value: Int, evidence: List[Evidence]}
## A row requiring measured or reviewed evidence beyond lexical source inventories.
export type Review = {name: Str, requirement: Str}
## Versioned report; semantic review rows cannot masquerade as measured structural metrics.
export type Report = {schema: Int, sources: List[SourceCount], metrics: List[Metric], reviews: List[Review]}
## A lexically validated Rust file, including module edges and exact structural occurrences.
export type SourceScan = {count: SourceCount, modules: List[RustModule], metrics: List[Metric]}

pure ident_start(value: Str) -> Bool {
  value != "" and (value in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_" or value.byte_len() > 1)
}
pure first(value: Str) -> Str { value.split("").get(0) ?? "" }
pure ident_part(value: Str) -> Bool { ident_start(value) or (value != "" and value in "0123456789") }
# Missing lookahead is an explicit end-of-input token.
const end_token: Token = {text: "", start: 0, end: 0, line: 0, last_line: 0, literal: false}
proc invalid(source_path: Str, line: Int, detail: Str) [error] -> Result[Unit] { Err(MetricError.Invalid(path: source_path, line: line, detail: detail)) }

# Literal contents never enter the token stream as identifiers or delimiters.
# Offsets are Unicode scalar indices; line ownership is independent of byte width.
proc lex(source_path: Str, source: Str) [error] -> Result[Lexed] {
  let chars = source.split("")
  var byte_offset = 0
  let offsets: List[Int] = collect {
    for scalar in chars { yield byte_offset; byte_offset += scalar.byte_len() }
    yield byte_offset
  }
  var cursor = 0
  var line = 1
  let tokens: List[Token] = collect {
    while cursor < chars.len() {
      let current = chars.get(cursor) ?? ""
      let next = chars.get(cursor + 1) ?? ""
      if current in [" ", "\t", "\r", "\n"] {
        if current == "\n" { line += 1 }
        cursor += 1
        continue
      }
      if current == "/" and next == "/" {
        while cursor < chars.len() and (chars.get(cursor) ?? "") != "\n" { cursor += 1 }
        continue
      }
      if current == "/" and next == "*" {
        let start_line = line
        cursor += 2
        var depth = 1
        while cursor < chars.len() and depth > 0 {
          if (chars.get(cursor) ?? "") == "/" and (chars.get(cursor + 1) ?? "") == "*" { depth += 1; cursor += 2 } else if (chars.get(cursor) ?? "") == "*" and (chars.get(cursor + 1) ?? "") == "/" { depth -= 1; cursor += 2 } else { if (chars.get(cursor) ?? "") == "\n" { line += 1 }; cursor += 1 }
        }
        if depth != 0 { invalid(source_path, start_line, "unterminated block comment")? }
        continue
      }
      let start = cursor
      let start_line = line
      if current == "r" and next == "#" and ident_start(chars.get(cursor + 2) ?? "") {
        cursor += 3
        while ident_part(chars.get(cursor) ?? "") { cursor += 1 }
        yield {text: source.byte_slice(offsets[start], length: offsets[cursor] - offsets[start]), start: start, end: cursor, line: start_line, last_line: line, literal: false}
        continue
      }
      var quote = cursor
      var raw = false
      if current in ["b", "c"] and next in ["\"", "'"] { quote += 1 }
      if current == "r" or (current in ["b", "c"] and next == "r") {
        quote = cursor + (if current == "r" { 1 } else { 2 })
        while (chars.get(quote) ?? "") == "#" { quote += 1 }
        if (chars.get(quote) ?? "") == "\"" { raw = true } else { quote = cursor }
      }
      let delimiter = chars.get(quote) ?? ""
      if current == "c" and quote > cursor and delimiter == "'" { invalid(source_path, line, "C character literal is unsupported")? }
      if raw and quote - start - (if current == "r" { 1 } else { 2 }) > 255 { invalid(source_path, line, "too many raw literal delimiters")? }
      var literal = delimiter == "\"" or (quote > cursor and delimiter == "'")
      if delimiter == "'" and quote == cursor {
        # A lifetime is an apostrophe followed by an identifier without a closing quote.
        if ident_start(next) {
          var end = cursor + 2
          while ident_part(chars.get(end) ?? "") { end += 1 }
          if (chars.get(end) ?? "") != "'" {
            cursor = end
            yield {text: source.byte_slice(offsets[start], length: offsets[cursor] - offsets[start]), start: start, end: cursor, line: start_line, last_line: line, literal: false}
            continue
          }
          if end != cursor + 2 { invalid(source_path, line, "multi-character Rust character literal")? }
        }
        literal = true
      }
      if literal {
        cursor = quote + 1
        var closed = false
        var payload = 0
        while cursor < chars.len() {
          let value = chars.get(cursor) ?? ""
          if value == delimiter {
            var end = cursor + 1
            if raw {
              let hashes = quote - start - (if current == "r" { 1 } else { 2 })
              var count = 0
              while count < hashes and (chars.get(end) ?? "") == "#" { count += 1; end += 1 }
              if count != hashes { cursor += 1; continue }
            }
            cursor = end
            closed = true
            break
          }
          if value == "\\" and ! raw {
            cursor += 1
            if cursor >= chars.len() { break }
            let escape = chars.get(cursor) ?? ""
            if ! (escape in ["\\", "\"", "'", "n", "r", "t", "0", "x", "u", "\n", "\r"]) { invalid(source_path, line, "unsupported Rust escape")? }
            if escape == "\n" { line += 1 }
            if delimiter == "'" and escape in ["\n", "\r"] { invalid(source_path, start_line, "newline in character escape")? }
            if escape == "u" {
              if current == "b" { invalid(source_path, line, "Unicode escape in byte literal")? }
              if (chars.get(cursor + 1) ?? "") != "{" { invalid(source_path, line, "malformed Unicode escape")? }
              cursor += 2
              var digits = 0
              var scalar = 0
              while cursor < chars.len() and (chars.get(cursor) ?? "") != "}" {
                let digit = chars.get(cursor) ?? ""
                if digit != "_" {
                  if ! (digit in "0123456789abcdefABCDEF") { invalid(source_path, line, "malformed Unicode escape")? }
                  digits += 1
                  scalar = scalar * 16 + ("0123456789abcdef".find(digit.lower()) ?? 0)
                }
                cursor += 1
              }
              if (chars.get(cursor) ?? "") != "}" or digits == 0 or digits > 6 { invalid(source_path, start_line, "malformed Unicode escape")? }
              if scalar > 1114111 or (scalar >= 55296 and scalar <= 57343) { invalid(source_path, line, "invalid Unicode scalar escape")? }
            } else if escape == "x" {
              for offset in [1, 2] {
                let digit = chars.get(cursor + offset) ?? ""
                if digit == "" or ! (digit in "0123456789abcdefABCDEF") { invalid(source_path, line, "malformed hexadecimal escape")? }
              }
              if current != "b" and current != "c" and (chars.get(cursor + 1) ?? "").lower() in "89abcdef" { invalid(source_path, line, "non-ASCII hexadecimal escape")? }
              cursor += 2
            }
            cursor += 1
            payload += 1
            continue
          }
          if current == "b" and value.byte_len() > 1 { invalid(source_path, line, "non-ASCII byte literal")? }
          if value == "\n" {
            if delimiter == "'" { invalid(source_path, start_line, "newline in character literal")? }
            line += 1
          }
          payload += 1
          cursor += 1
        }
        if ! closed { invalid(source_path, start_line, "unterminated Rust literal")? }
        if delimiter == "'" and payload != 1 { invalid(source_path, start_line, "invalid character literal")? }
      } else {
        cursor += 1
        if ident_start(current) {
          while ident_part(chars.get(cursor) ?? "") { cursor += 1 }
        }
      }
      yield {text: source.byte_slice(offsets[start], length: offsets[cursor] - offsets[start]), start: start, end: cursor, line: start_line, last_line: line, literal: literal}
    }
  }
  var stack: List[Int] = []
  var mates: Map[Int] = {}
  var token_table: Map[Int, Token] = {}
  var index = 0
  for item in tokens {
    token_table[index] = item
    if ! item.literal and item.text in ["(", "[", "{"] { stack += [index] }
    if ! item.literal and item.text in [")", "]", "}"] {
      if stack.is_empty() { invalid(source_path, item.line, "unmatched closing delimiter")? }
      let opening = stack[stack.len() - 1]
      let expected = if item.text == ")" { "(" } else if item.text == "]" { "[" } else { "{" }
      if tokens[opening].text != expected { invalid(source_path, item.line, "mismatched Rust delimiters")? }
      mates[f"{opening}"] = index
      mates[f"{index}"] = opening
      stack = stack |> take(stack.len() - 1) |> collect
    }
    index += 1
  }
  if ! stack.is_empty() { invalid(source_path, tokens[stack[0]].line, "unclosed Rust delimiter")? }
  let physical = if source == "" { 0 } else { line - (if source.ends_with("\n") { 1 } else { 0 }) }
  {tokens: token_table, mates: mates, size: chars.len(), lines: physical}
}

pure words(lexed: Lexed, start: Int, end: Int) -> Str {
  let values = collect {
    var index = start
    while index < end { yield lexed.tokens[index].text; index += 1 }
  }
  values.join("")
}

proc cfg_eval(source_path: Str, lexed: Lexed, start: Int, assignments: Map[Bool]) [error] -> Result[CfgValue] {
  let name = (lexed.tokens.get(start) ?? end_token).text
  if name in ["all", "any", "not"] and (lexed.tokens.get(start + 1) ?? end_token).text == "(" {
    let end = lexed.mates[f"{start + 1}"]
    var cursor = start + 2
    var value = name != "any"
    var count = 0
    while cursor < end {
      let child = cfg_eval(source_path, lexed, cursor, assignments)?
      if name == "any" { value = value or child.value } else if name == "all" { value = value and child.value } else { value = ! child.value }
      count += 1
      cursor = child.next
      if cursor < end {
        if (lexed.tokens.get(cursor) ?? end_token).text != "," { invalid(source_path, lexed.tokens[cursor].line, "expected comma in cfg")? }
        cursor += 1
      }
    }
    if name == "not" and count != 1 { invalid(source_path, lexed.tokens[start].line, "cfg not requires one predicate")? }
    return {value: value, next: end + 1}
  }
  if ! ident_start(first(name)) { invalid(source_path, lexed.tokens[start].line, "unsupported cfg predicate")? }
  var end = start + 1
  if (lexed.tokens.get(end) ?? end_token).text == "=" {
    if ! lexed.tokens[end + 1].literal { invalid(source_path, lexed.tokens[start].line, "cfg value must be a literal")? }
    end += 2
  }
  let atom = words(lexed, start, end)
  {value: if atom == "test" { false } else { assignments.get(atom) ?? false }, next: end}
}

proc satisfiable(source_path: Str, lexed: Lexed, predicates: List[CfgPredicate], atoms: List[Str], index: Int, assignments: Map[Bool]) [error] -> Result[Bool] {
  if index < atoms.len() {
    return true when satisfiable(source_path, lexed, predicates, atoms, index + 1, assignments.set(atoms[index], false))?
    return satisfiable(source_path, lexed, predicates, atoms, index + 1, assignments.set(atoms[index], true))?
  }
  for predicate in predicates {
    let value = cfg_eval(source_path, lexed, predicate.start, assignments)?
    if value.next != predicate.end { invalid(source_path, lexed.tokens[predicate.start].line, "trailing cfg tokens")? }
    return false when ! value.value
  }
  true
}

proc production_cfg(source_path: Str, lexed: Lexed, predicates: List[CfgPredicate]) [error] -> Result[Bool] {
  # Recursive boolean evaluation needs only these predicates, not every
  # source token. Original token indices preserve diagnostic evidence.
  var cfg_tokens: Map[Int, Token] = {}
  var cfg_mates: Map[Int] = {}
  for predicate in predicates {
    var index = predicate.start - 1
    while index <= predicate.end {
      cfg_tokens[index] = lexed.tokens[index]
      if let Ok(other) = lexed.mates.get(f"{index}") { cfg_mates[f"{index}"] = other }
      index += 1
    }
  }
  let cfg_source: Lexed = {tokens: cfg_tokens, mates: cfg_mates, size: lexed.size, lines: lexed.lines}
  var atoms: List[Str] = []
  for predicate in predicates {
    let end = predicate.end
    var cursor = predicate.start
    while cursor < end {
      let name = (lexed.tokens.get(cursor) ?? end_token).text
      if (lexed.tokens.get(cursor + 1) ?? end_token).text == "(" or name in ["(", ")", ","] { cursor += 1; continue }
      var next = cursor + 1
      if (lexed.tokens.get(next) ?? end_token).text == "=" { next += 2 }
      let atom = words(cfg_source, cursor, next)
      if atom != "test" and ! (atom in atoms) { atoms += [atom] }
      cursor = next
    }
  }
  satisfiable(source_path, cfg_source, predicates, atoms, 0, {})?
}

proc attributes(source_path: Str, lexed: Lexed) [error] -> Result[List[Attribute]] {
  var cursor = 0
  var scopes: List[Int] = []
  let found: List[Attribute] = collect {
    while cursor < lexed.tokens.len() {
      let name = (lexed.tokens.get(cursor) ?? end_token).text
      if name == "{" { scopes += [cursor] }
      if name == "}" { scopes = scopes |> take(scopes.len() - 1) |> collect }
      if name == "#" {
        let inner = (lexed.tokens.get(cursor + 1) ?? end_token).text == "!"
        let opening = cursor + (if inner { 2 } else { 1 })
        if (lexed.tokens.get(opening) ?? end_token).text != "[" { invalid(source_path, lexed.tokens[cursor].line, "unsupported Rust attribute")? }
        let end = lexed.mates[f"{opening}"]
        let kind = (lexed.tokens.get(opening + 1) ?? end_token).text
        var condition_active = true
        if kind in ["cfg", "cfg_attr"] and ((lexed.tokens.get(opening + 2) ?? end_token).text != "(" or lexed.mates[f"{opening + 2}"] != end - 1) {
          invalid(source_path, lexed.tokens[cursor].line, "malformed conditional attribute")?
        }
        if kind == "cfg_attr" {
          # Conditional metadata is harmless; conditional cfg/source_path or custom
          # attributes could alter item ownership and must be reviewed explicitly.
          var child = opening + 3
          while child < end {
            if (lexed.tokens.get(child) ?? end_token).text in ["(", "[", "{"] { child = lexed.mates[f"{child}"] + 1; continue }
            if (lexed.tokens.get(child) ?? end_token).text == "," { break }
            child += 1
          }
          if (lexed.tokens.get(child) ?? end_token).text != "," { invalid(source_path, lexed.tokens[cursor].line, "cfg_attr requires metadata")? }
          condition_active = production_cfg(source_path, lexed, [{start: opening + 3, end: child}])?
          child += 1
          while child < end - 1 {
            if ! ((lexed.tokens.get(child) ?? end_token).text in ["allow", "warn", "deny", "forbid", "expect", "derive", "doc"]) {
              invalid(source_path, lexed.tokens[cursor].line, "cfg_attr eligibility is unsupported")?
            }
            child += 1
            if (lexed.tokens.get(child) ?? end_token).text == "(" { child = lexed.mates[f"{child}"] + 1 } else if (lexed.tokens.get(child) ?? end_token).text == "=" { child += 2 } else { invalid(source_path, lexed.tokens[cursor].line, "malformed cfg_attr metadata")? }
            if (lexed.tokens.get(child) ?? end_token).text == "," { child += 1 }
          }
        }
        yield {start: cursor, end: end, inner: inner, scope: scopes.get(scopes.len() - 1) ?? -1, condition_active: condition_active}
        cursor = end + 1
      } else { cursor += 1 }
    }
  }
  found
}

proc item_end(source_path: Str, lexed: Lexed, start: Int) [error] -> Result[Int] {
  var cursor = start
  var semicolon = false
  var angles = 0
  var declaration = false
  while cursor < lexed.tokens.len() {
    let name = (lexed.tokens.get(cursor) ?? end_token).text
    # Signature keywords such as const parameters do not change item kind.
    if ! declaration and name in ["fn", "mod", "struct", "enum", "impl", "trait", "union", "extern", "use", "const", "static", "type", "macro_rules"] {
      if name == "const" and (lexed.tokens.get(cursor + 1) ?? end_token).text in ["fn", "unsafe", "async"] { cursor += 1; continue }
      declaration = true
      semicolon = name in ["use", "static", "type", "const"]
    }
    if name == "<" and ! semicolon { angles += 1 }
    if name == ">" and angles > 0 and (lexed.tokens.get(cursor - 1) ?? end_token).text != "-" { angles -= 1 }
    if name == ";" { return cursor }
    if name == "," and angles == 0 and ! declaration { return cursor }
    if name == "{" {
      let end = lexed.mates[f"{cursor}"]
      # A use tree is followed by its semicolon; its braces do not end the item.
      if semicolon or angles > 0 { cursor = end + 1; continue }
      return end + (if (lexed.tokens.get(end + 1) ?? end_token).text in [";", ","] { 1 } else { 0 })
    }
    if name in ["(", "["] { cursor = lexed.mates[f"{cursor}"] + 1 } else { cursor += 1 }
  }
  invalid(source_path, lexed.tokens[start].line, "cannot determine attributed item boundary")?
  start
}

pure inside(item: Token, spans: List[Span]) -> Bool {
  for span in spans { return true when item.start >= span.start and item.end <= span.end }
  false
}

proc scope_start(source_path: Str, lexed: Lexed, opening: Int) [error] -> Result[Int] {
  var start = opening
  var cursor = opening - 1
  var item = false
  while cursor >= 0 {
    let name = (lexed.tokens.get(cursor) ?? end_token).text
    if name in [";", "{", "}"] { break }
    if name in ["fn", "mod", "impl", "trait", "extern"] { item = true }
    if name in [")", "]"] { cursor = lexed.mates[f"{cursor}"] }
    start = cursor
    cursor -= 1
  }
  if ! item { invalid(source_path, lexed.tokens[opening].line, "inner cfg scope is not a supported Rust item")? }
  start
}

proc excluded_spans(source_path: Str, lexed: Lexed, attrs: List[Attribute]) [error] -> Result[List[Span]] {
  var spans: List[Span] = []
  var cursor = 0
  while cursor < attrs.len() {
    let first = attrs[cursor]
    var last = first
    var predicates: List[CfgPredicate] = []
    while true {
      let actual = last.start + (if last.inner { 3 } else { 2 })
      if (lexed.tokens.get(actual) ?? end_token).text == "cfg" {
        if (lexed.tokens.get(actual + 1) ?? end_token).text != "(" { invalid(source_path, lexed.tokens[last.start].line, "malformed cfg attribute")? }
        predicates += [{start: actual + 2, end: lexed.mates[f"{actual + 1}"]}]
      }
      cursor += 1
      if cursor >= attrs.len() or attrs[cursor].start != last.end + 1 or attrs[cursor].inner != first.inner { break }
      last = attrs[cursor]
    }
    if ! predicates.is_empty() and ! production_cfg(source_path, lexed, predicates)? {
      var start = lexed.tokens[first.start].start
      var end = lexed.size
      var start_line = lexed.tokens[first.start].line
      var last_line = lexed.lines
      if first.inner {
        if first.scope >= 0 {
          let item = scope_start(source_path, lexed, first.scope)?
          start = lexed.tokens[item].start
          start_line = lexed.tokens[item].line
          let closing = lexed.mates[f"{first.scope}"]
          end = lexed.tokens[closing].end
          last_line = lexed.tokens[closing].last_line
        } else { start = 0; start_line = 1 }
      } else {
        let closing = item_end(source_path, lexed, last.end + 1)?
        end = lexed.tokens[closing].end
        last_line = lexed.tokens[closing].last_line
      }
      spans += [{start: start, end: end, line: start_line, last_line: last_line}]
    }
  }
  spans
}

pure count_lines(source_path: Str, lexed: Lexed, spans: List[Span]) -> SourceCount {
  var retained: Map[Bool] = {}
  var excluded: Map[Bool] = {}
  for span in spans {
    var line = span.line
    while line <= span.last_line { excluded[f"{line}"] = true; line += 1 }
  }
  for item in lexed.tokens.values() {
    if ! inside(item, spans) {
      var line = item.line
      while line <= item.last_line { retained[f"{line}"] = true; line += 1 }
    }
  }
  let lines: List[Int] = collect {
    var line = 1
    while line <= lexed.lines {
      yield line when ! (excluded.get(f"{line}") ?? false) or (retained.get(f"{line}") ?? false)
      line += 1
    }
  }
  {path: source_path, physical: lexed.lines, production: lines.len(), retained_lines: lines, excluded: spans}
}

proc modules(source_path: Str, lexed: Lexed, attrs: List[Attribute], spans: List[Span]) [error] -> Result[List[RustModule]] {
  let file = fp"{source_path}"
  let stem = file.name().byte_slice(0, length: file.name().byte_len() - 3)
  let directory = if stem in ["lib", "main", "mod"] { file.parent().display() } else { f"{file.parent()}/{stem}" }
  var inline: List[InlineModule] = []
  var cursor = 0
  let edges: List[RustModule] = collect {
    while cursor < lexed.tokens.len() {
      while ! inline.is_empty() and cursor > inline[inline.len() - 1].end { inline = inline |> take(inline.len() - 1) |> collect }
      if (lexed.tokens.get(cursor) ?? end_token).text == "mod" and ident_start(first((lexed.tokens.get(cursor + 1) ?? end_token).text)) {
        let name = (lexed.tokens.get(cursor + 1) ?? end_token).text
        let parent = if inline.is_empty() { directory } else { inline[inline.len() - 1].directory }
        var explicit: Str? = null
        var previous = cursor - 1
        if (lexed.tokens.get(previous) ?? end_token).text == ")" { previous = lexed.mates[f"{previous}"] - 1 }
        if (lexed.tokens.get(previous) ?? end_token).text == "pub" { previous -= 1 }
        let backwards = collect {
          var index = attrs.len() - 1
          while index >= 0 { yield attrs[index]; index -= 1 }
        }
        for attr in backwards {
          if attr.end != previous { continue }
          let kind = attr.start + (if attr.inner { 3 } else { 2 })
          if (lexed.tokens.get(kind) ?? end_token).text == "path" {
            let value = (lexed.tokens.get(kind + 2) ?? end_token).text
            if (lexed.tokens.get(kind + 1) ?? end_token).text != "=" { invalid(source_path, lexed.tokens[attr.start].line, "malformed path attribute")? }
            if value.starts_with("r") and "\"" in value {
              let parts = value.split("\"", maxsplit: 1)
              let suffix = f"\"{parts[0].byte_slice(1)}"
              explicit = parts[1].byte_slice(0, length: parts[1].byte_len() - suffix.byte_len())
            } else {
              if ! value.starts_with("\"") or ! value.ends_with("\"") or "\\" in value { invalid(source_path, lexed.tokens[attr.start].line, "unsupported path attribute literal")? }
              explicit = value.byte_slice(1, length: value.byte_len() - 2)
            }
          }
          previous = attr.start - 1
        }
        # An explicit inline directory replaces the normal module component.
        let base = if inline.is_empty() { file.parent().display() } else { parent }
        if (lexed.tokens.get(cursor + 2) ?? end_token).text == "{" {
          inline += [{end: lexed.mates[f"{cursor + 2}"], directory: if explicit != null { f"{base}/{explicit}" } else { f"{parent}/{name}" }}]
        }
        if (lexed.tokens.get(cursor + 2) ?? end_token).text == ";" {
          yield {name: name, directory: if explicit != null { base } else { parent }, explicit_path: explicit, excluded: inside(lexed.tokens[cursor], spans), dead_code: false, line: lexed.tokens[cursor].line}
        }
      }
      cursor += 1
    }
  }
  edges
}

pure metric(name: Str, evidence: List[Evidence]) -> Metric { {name: name, value: evidence.len(), evidence: evidence} }
pure evidence(source_path: Str, item: Token, detail: Str) -> Evidence { {path: source_path, line: item.line, detail: detail} }

proc structural(source_path: Str, lexed: Lexed, spans: List[Span], attrs: List[Attribute], count: SourceCount) [error] -> Result[List[Metric]] {
  let families = ["ArenaExprKind", "ArenaStmtKind", "ArenaPatternKind"]
  var aliases: Map[Str] = {}
  var calls: List[Evidence] = []
  var optional: List[Evidence] = []
  var instructions: List[Evidence] = []
  var patterns: List[Evidence] = []
  var stages: List[Evidence] = []
  var wildcards: List[Evidence] = []
  var predicates: List[Evidence] = []
  var mappings: List[Evidence] = []
  var resolvers: List[Evidence] = []
  var factories: List[Evidence] = []
  var blanket: List[Evidence] = []
  var cursor = 0
  while cursor < lexed.tokens.len() {
    let item = lexed.tokens[cursor]
    let name = item.text
    if ! item.literal and ! inside(item, spans) {
      if name in families and (lexed.tokens.get(cursor + 1) ?? end_token).text == "as" { aliases[(lexed.tokens.get(cursor + 2) ?? end_token).text] = name }
      if name in ["indexed_raw", "indexed_decode", "indexed_finish", "indexed_optional_raw"] and (lexed.tokens.get(cursor - 1) ?? end_token).text != "fn" {
        let next = (lexed.tokens.get(cursor + 1) ?? end_token).text
        if next == "(" or (next == ":" and (lexed.tokens.get(cursor + 2) ?? end_token).text == ":" and (lexed.tokens.get(cursor + 3) ?? end_token).text == "<") {
          if name == "indexed_optional_raw" { optional += [evidence(source_path, item, name)] } else { calls += [evidence(source_path, item, name)] }
        }
      }
      if name == "fn" {
        let definition = (lexed.tokens.get(cursor + 1) ?? end_token).text
        if definition in ["value_matches_static_type", "lowered_value_matches_static_type", "test_value_matches_type"] { predicates += [evidence(source_path, item, definition)] }
        if definition in ["lowered_checked_type", "type_for_lowered_type", "lowered_type_to_type", "lowered_type_from_type"] { mappings += [evidence(source_path, item, definition)] }
        if definition in ["lowered_arena_type_inner", "compact_runtime_type_inner", "compact_type_check"] { resolvers += [evidence(source_path, item, definition)] }
      }
      if name == "enum" and (lexed.tokens.get(cursor + 2) ?? end_token).text == "{" {
        let definition = (lexed.tokens.get(cursor + 1) ?? end_token).text
        if definition in ["BuildIntRow", "BuildBoolRow", "BuildExprRow", "BuildStmtRow", "BuildPatternRow", "LoweredPipelineStage"] {
          let closing = lexed.mates[f"{cursor + 2}"]
          var member = cursor + 3
          var at_start = true
          while member < closing {
            let value = (lexed.tokens.get(member) ?? end_token).text
            if value == "#" { member = lexed.mates[f"{member + 1}"] + 1; continue }
            if at_start and ident_start(first(value)) and ! inside(lexed.tokens[member], spans) {
              let found = evidence(source_path, lexed.tokens[member], f"{definition}::{value}")
              if definition == "BuildPatternRow" { patterns += [found] } else if definition == "LoweredPipelineStage" { stages += [found] } else { instructions += [found] }
              at_start = false
            }
            if value in ["(", "[", "{"] { member = lexed.mates[f"{member}"] + 1; continue }
            if value == "," { at_start = true }
            member += 1
          }
        }
      }
      if name == "match" {
        var opening = cursor + 1
        while opening < lexed.tokens.len() and (lexed.tokens.get(opening) ?? end_token).text != "{" {
          if (lexed.tokens.get(opening) ?? end_token).text in ["(", "["] { opening = lexed.mates[f"{opening}"] + 1 } else { opening += 1 }
        }
        if opening >= lexed.tokens.len() { invalid(source_path, item.line, "match body not found")? }
        let closing = lexed.mates[f"{opening}"]
        var member = opening + 1
        var in_pattern = true
        var names: List[Str] = []
        var wildcard = false
        var last_wildcard = false
        while member < closing {
          let value = (lexed.tokens.get(member) ?? end_token).text
          if value == "=" and (lexed.tokens.get(member + 1) ?? end_token).text == ">" {
            last_wildcard = wildcard
            wildcard = false
            in_pattern = false
            member += 2
            continue
          }
          if in_pattern {
            if value == "_" and (lexed.tokens.get(member + 1) ?? end_token).text == "=" and (lexed.tokens.get(member + 2) ?? end_token).text == ">" { wildcard = true }
            let family = aliases.get(value) ?? value
            if family in families and (lexed.tokens.get(member + 1) ?? end_token).text == ":" and (lexed.tokens.get(member + 2) ?? end_token).text == ":" {
              let variant = f"{family}::{(lexed.tokens.get(member + 3) ?? end_token).text}"
              if ! (variant in names) { names += [variant] }
            }
          }
          if value in ["(", "[", "{"] {
            let end = lexed.mates[f"{member}"]
            if in_pattern {
              var nested = member + 1
              while nested < end {
                let head = lexed.tokens[nested].text
                let family = aliases.get(head) ?? head
                if family in families and (lexed.tokens.get(nested + 1) ?? end_token).text == ":" and (lexed.tokens.get(nested + 2) ?? end_token).text == ":" {
                  let variant = f"{family}::{lexed.tokens[nested + 3].text}"
                  if ! (variant in names) { names += [variant] }
                }
                nested += 1
              }
            }
            member = end + 1
            if ! in_pattern and value == "{" { in_pattern = true }
            continue
          }
          if value == "," { in_pattern = true }
          member += 1
        }
        if last_wildcard and names.len() >= 8 { wildcards += [evidence(source_path, item, names.join(","))] }
      }
      if name == "ExplicitFrames" and (lexed.tokens.get(cursor + 1) ?? end_token).text == ":" and (lexed.tokens.get(cursor + 2) ?? end_token).text == ":" and (lexed.tokens.get(cursor + 3) ?? end_token).text == "new" and (lexed.tokens.get(cursor + 4) ?? end_token).text == "(" {
        factories += [evidence(source_path, item, name)]
      }
    }
    cursor += 1
  }
  for attr in attrs {
    if inside(lexed.tokens[attr.start], spans) or ! attr.condition_active { continue }
    let kind = attr.start + (if attr.inner { 3 } else { 2 })
    var allowed = false
    var member = kind
    while member < attr.end {
      if (lexed.tokens.get(member) ?? end_token).text == "allow" and (lexed.tokens.get(member + 1) ?? end_token).text == "(" {
        let end = lexed.mates[f"{member + 1}"]
        var argument = member + 2
        while argument < end { if (lexed.tokens.get(argument) ?? end_token).text == "dead_code" { allowed = true }; argument += 1 }
      }
      member += 1
    }
    if allowed {
      let last = if attr.inner { if attr.scope < 0 { lexed.lines } else { lexed.tokens[lexed.mates[f"{attr.scope}"]].line } } else { lexed.tokens[item_end(source_path, lexed, attr.end + 1)?].line }
      let first = if attr.inner { if attr.scope < 0 { 1 } else { lexed.tokens[scope_start(source_path, lexed, attr.scope)?].line } } else { lexed.tokens[attr.start].line }
      for line in count.retained_lines { if line >= first and line <= last { blanket += [{path: source_path, line: line, detail: "allow(dead_code) scope"}] } }
    }
  }
  # Overlapping blanket scopes own a physical line only once.
  var seen: Map[Bool] = {}
  let unique = collect {
    for found in blanket { if ! (seen.get(f"{found.line}") ?? false) { yield found; seen[f"{found.line}"] = true } }
  }
  [metric("instruction_variants", instructions), metric("pattern_variants", patterns), metric("stage_variants", stages), metric("manual_payload_calls", calls), metric("optional_raw_calls", optional), metric("wide_arena_wildcards", wildcards), metric("static_type_predicate_definitions", predicates), metric("type_storage_mapping_definitions", mappings), metric("lowering_type_resolver_definitions", resolvers), metric("nested_machine_factory_calls", factories), metric("blanket_dead_code_lines", unique)]
}

## Scans one physical source file without executing Rust or inspecting literal text as code.
export proc scan_file(source_path: Str, source: Str) [error] -> Result[SourceScan, Error] {
  let lexed = lex(source_path, source)?
  let attrs = attributes(source_path, lexed)?
  let spans = excluded_spans(source_path, lexed, attrs)?
  let count = count_lines(source_path, lexed, spans)
  let metrics = structural(source_path, lexed, spans, attrs, count)?
  let blanket = metrics |> where .name == "blanket_dead_code_lines" |> collect
  let allowed = blanket[0].evidence |> map .line |> collect
  let edges = collect {
    for edge in modules(source_path, lexed, attrs, spans)? {
      yield {name: edge.name, directory: edge.directory, explicit_path: edge.explicit_path, excluded: edge.excluded, dead_code: edge.line in allowed, line: edge.line}
    }
  }
  {count: count, modules: edges, metrics: metrics}
}

proc module_target(root: Path, owner: Str, edge: RustModule) [fs, error] -> Result[Str] {
  var candidates: List[Path] = []
  if edge.explicit_path != null { candidates = [if edge.explicit_path.starts_with("/") { fp"{edge.explicit_path}" } else { fp"{root}/{edge.directory}/{edge.explicit_path}" }] } else { candidates = [fp"{root}/{edge.directory}/{edge.name}.rs", fp"{root}/{edge.directory}/{edge.name}/mod.rs"] }
  let existing = collect { for candidate in candidates { yield candidate.resolve()?.strip_prefix(root)?.display() when candidate.exists()? } }
  if existing.len() != 1 { return Err(MetricError.Invalid(path: owner, line: edge.line, detail: f"module {edge.name} resolves to {existing.len()} files")) }
  existing[0]
}

## Measures both production source roots; external test modules are removed through their module edges.
export proc measure(root: Path, source_roots: List[Str]) [fs, error] -> Result[Report, Error] {
  if (source_roots |> sort) != ["crates/xsht/src", "src"] { return Err(MetricError.Contract(detail: "source roots must be exactly src and crates/xsht/src")) }
  let base = root.resolve()?
  var scans: List[SourceScan] = []
  var sizes: Map[Int] = {}
  var referenced: Map[Bool] = {}
  var active: Map[Bool] = {}
  var inherited: Map[Bool] = {}
  var edges: List[ModuleEdge] = []
  for source_root in source_roots {
    for entry in fs.walk(fp"{base}/{source_root}")? |> where .kind == "file" and .path.ext() == "rs" |> sort-by .path {
      let source_path = entry.path.strip_prefix(base)?.display()
      let source = entry.path.read_text()?
      let scan = scan_file(source_path, source)?
      sizes[source_path] = source.count_chars()
      scans += [scan]
      for edge in scan.modules {
        let target = module_target(base, source_path, edge)?
        referenced[target] = true
        edges += [{owner: source_path, target: target, excluded: edge.excluded, dead_code: edge.dead_code}]
      }
    }
  }
  for scan in scans { if ! (referenced.get(scan.count.path) ?? false) { active[scan.count.path] = true } }
  var changed = true
  while changed {
    changed = false
    for edge in edges {
      if ! edge.excluded and (active.get(edge.owner) ?? false) and ! (active.get(edge.target) ?? false) { active[edge.target] = true; changed = true }
      if ! edge.excluded and (active.get(edge.owner) ?? false) and (edge.dead_code or (inherited.get(edge.owner) ?? false)) and ! (inherited.get(edge.target) ?? false) { inherited[edge.target] = true; changed = true }
    }
  }
  var sources: List[SourceCount] = []
  var totals: Map[Int] = {}
  var occurrences: Map[List[Evidence]] = {}
  for scan in scans {
    let live = active.get(scan.count.path) ?? false
    let count = if live { scan.count } else { {path: scan.count.path, physical: scan.count.physical, production: 0, retained_lines: [], excluded: [{start: 0, end: sizes[scan.count.path], line: 1, last_line: scan.count.physical}]} }
    sources += [count]
    let source_group = if count.path.starts_with("src/") { "source_lines_src" } else { "source_lines_xsht" }
    totals[source_group] = (totals.get(source_group) ?? 0) + count.production
    occurrences[source_group] = (occurrences.get(source_group) ?? []) + [{path: count.path, line: 1, detail: f"{count.production} retained physical lines"}]
    for row in scan.metrics {
      let found = if row.name == "blanket_dead_code_lines" and (inherited.get(count.path) ?? false) {
        collect { for line in count.retained_lines { yield {path: count.path, line: line, detail: "inherited allow(dead_code) module scope"} } }
      } else { row.evidence }
      let amount = if row.name == "blanket_dead_code_lines" { found.len() } else { row.value }
      totals[row.name] = (totals.get(row.name) ?? 0) + (if live { amount } else { 0 })
      occurrences[row.name] = (occurrences.get(row.name) ?? []) + (if live { found } else { [] })
    }
  }
  let metrics = collect { for name in totals.keys() |> sort { yield {name: name, value: totals[name], evidence: occurrences[name]} } }
  {schema: 1, sources: sources, metrics: metrics, reviews: [
    {name: "operand_behavior_differences", requirement: "Verified per-instruction behavior comparison and retained oracle tests; source arm counts cannot establish equivalence."},
    {name: "duplicated_per_item_stages", requirement: "Reviewed stage execution ownership, backed by verified generated stage tables and pipeline differential probes."},
    {name: "nested_machine_bounds", requirement: "Review each entry call and constructor for native-stack nesting and the accepted counter bound; lexical calls alone do not prove scheduling."},
    {name: "instruction_fallback_arms", requirement: "Verified generated instruction dispatch inventory identifying recursive fallback routes."},
    {name: "name_resolution_authorities", requirement: "Reviewed authority boundaries and feature-touch evidence; source definitions do not prove semantic authority."},
    {name: "lint_recursive_descents", requirement: "Verified traversal inventory and rule probes identifying independent recursive descents."},
    {name: "module_checks_per_pass", requirement: "Measured single lint-pass module-check counts with the shared-import fixture."},
    {name: "whole_program_stage_copies", requirement: "Reviewed stage-lowering ownership and allocation probes; arbitrary clone calls are not a semantic count."},
    {name: "open_defects", requirement: "Reviewed TODO defect dispositions with retained regression coverage and recorded reasons."},
    {name: "timings", requirement: "Isolated repository lint and full gate wall-clock measurements; timings are evidence without a rise ratchet."},
  ]}
}

## Fails on a structural rise or a changed metric schema; baseline files decode through Report.
export proc assert_not_risen(current: Report, baseline: Report) [error] -> Result[Unit, Error] {
  if current.schema != baseline.schema { return Err(MetricError.Contract(detail: "metric schema changed")) }
  let names = current.metrics |> map .name |> sort
  let previous = baseline.metrics |> map .name |> sort
  if names != previous { return Err(MetricError.Contract(detail: "metric inventory changed")) }
  for row in current.metrics {
    let old = baseline.metrics |> where .name == row.name |> collect
    if old.len() != 1 { return Err(MetricError.Contract(detail: f"duplicate metric {row.name}")) }
    if row.value < 0 or old[0].value < 0 { return Err(MetricError.Contract(detail: f"negative metric {row.name}")) }
    if row.value > old[0].value { return Err(MetricError.Rise(name: row.name, before: old[0].value, after: row.value)) }
  }
}

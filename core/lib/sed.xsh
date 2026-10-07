##! Parsed stream editing with byte records and POSIX regular expressions.
use gnu

error SedError = Invalid : Usage

type RegexSpec = {pattern: Str, insensitive: Bool, groups: Int}
enum RegexChoice { Explicit(RegexSpec), Previous }
enum Address { Line(Int), Stride(Int, Int), Last, Relative(Int), PatternAddress(RegexChoice) }
enum Primitive {
  Delete, DeleteFirst, Print, PrintFirst, Store, AppendHold, Load,
  AppendPattern, Exchange, Next, AppendNext, Quit, QuitQuiet, Number, Clear
}
type Substitution = {regex: RegexChoice, replacement: Bytes, global: Bool, occurrence: Int, print: Bool}
enum TextKind { AppendText, InsertText, ChangeText }
type TextCommand = {kind: TextKind, text: Bytes}
enum BranchKind { Always, OnSubstitution, WithoutSubstitution }
type BranchCommand = {kind: BranchKind, label: Str}
type Translation = {from: Bytes, to: Bytes}
enum Action {
  Simple(Primitive), Text(TextCommand), Branch(BranchCommand), Label(Str),
  BlockEnd(Int), End, Substitute(Substitution), Translate(Translation)
}
type EditCommand = {first: Address?, last: Address?, invert: Bool, action: Action}
type Program = {commands: List[EditCommand], labels: Map[Int], extended: Bool, quiet: Bool}
type BytesScan = {value: Bytes, next: Int}
type NumberScan = {value: Int, next: Int}
type RegexScan = {value: RegexChoice, previous: RegexSpec}
type AddressScan = {value: Address?, next: Int, previous: RegexSpec?}
type EditRecord = {text: Bytes, newline: Bool}
type Output = {parts: List[Bytes], separated: Bool}
type Selection = {selected: Bool, ending: Bool, active: Int?, previous: RegexSpec?}
type AddressMatch = {matched: Bool, previous: RegexSpec?}
type Replacement = {text: Bytes, changed: Bool}
type Capture = {start: Int, end: Int}

pure digit(byte: Int, radix: Int) -> Int? {
  let value = if byte >= 48 and byte <= 57 { byte - 48 } else if byte >= 65 and byte <= 70 { byte - 55 } else if byte >= 97 and byte <= 102 { byte - 87 } else { -1 }
  if value >= 0 and value < radix { value } else { null }
}

pure space(input: Bytes, start: Int) -> Int {
  var at = start
  while (input.byte_at(at) ?? -1) in [32, 9, 13] { at += 1 }
  at
}

pure number(input: Bytes, start: Int) -> Result[NumberScan, Error] {
  var at = start
  while (input.byte_at(at) ?? -1) >= 48 and (input.byte_at(at) ?? -1) <= 57 { at += 1 }
  let text = input[start..at].utf8()?
  match text.parse_int() {
    Ok(value) => Ok({value: value, next: at})
    Err(_) => Err(SedError.Invalid("invalid numeric address or flag"))
  }
}

pure control(byte: Int) -> Int? {
  match byte {
    110 => 10
    116 => 9
    114 => 13
    97 => 7
    102 => 12
    118 => 11
    _ => null
  }
}

pure delimited(input: Bytes, start: Int, delimiter: Int, replacement: Bool) -> Result[BytesScan, Error] {
  var at = start
  var out: List[Bytes] = []
  while at < input.len() {
    let ch = input.byte_at(at) ?? -1
    at += 1
    if ch == delimiter { return Ok({value: bytes.concat(out), next: at}) }
    if ch == 10 { return Err(SedError.Invalid("unterminated delimited expression")) }
    if ch != 92 { out += [input[at - 1..at]]; continue }
    if at == input.len() { return Err(SedError.Invalid("unterminated escape")) }
    let next = input.byte_at(at) ?? -1
    at += 1
    if next in [120, 111, 100] {
      let radix = if next == 120 { 16 } else if next == 111 { 8 } else { 10 }
      let width = if radix == 16 { 2 } else { 3 }
      var value = 0
      var digits = 0
      while digits < width {
        let found = digit(input.byte_at(at) ?? -1, radix)
        if found == null { break }
        value = value * radix + (found ?? 0)
        at += 1
        digits += 1
      }
      if digits > 0 {
        let byte = value % 256
        if replacement and byte in [38, 92] { out += [b"\\"] }
        out += [bytes.from_ints([byte])?]
        continue
      }
    }
    if next == delimiter {
      if replacement and next in [38, 92] { out += [b"\\"] }
      out += [input[at - 1..at]]
    } else if next == 10 {
      out += [b"\n"]
    } else if !replacement and control(next) != null {
      out += [bytes.from_ints([control(next) ?? 0])?]
    } else {
      out += [b"\\", input[at - 1..at]]
    }
  }
  Err(SedError.Invalid("unterminated delimited expression"))
}

pure group_count(pattern: Bytes, extended: Bool) -> Int {
  var count = 0
  var bracket = false
  var at = 0
  while at < pattern.len() {
    let byte = pattern.byte_at(at) ?? -1
    at += 1
    if byte == 92 {
      if at < pattern.len() {
        if !bracket and !extended and pattern.byte_at(at) == 40 { count += 1 }
        at += 1
      }
    } else if byte == 91 { bracket = true } else if byte == 93 { bracket = false } else if !bracket and extended and byte == 40 { count += 1 }
  }
  count
}

pure make_regex(pattern: Bytes, previous: RegexSpec?, extended: Bool, insensitive: Bool) -> Result[RegexScan, Error] {
  if pattern.is_empty() {
    guard let saved = previous else { return Err(SedError.Invalid("no previous regular expression")) }
    return Ok({value: .Previous, previous: saved})
  }
  let text = pattern.utf8()?
  let validated = regex.captures_bytes(text, b"", extended: extended, ignore_case: insensitive)?
  let spec: RegexSpec = {pattern: text, insensitive: insensitive, groups: group_count(pattern, extended)}
  Ok({value: .Explicit(spec), previous: spec})
}

pure address(input: Bytes, start: Int, second: Bool, previous: RegexSpec?, extended: Bool) -> Result[AddressScan, Error] {
  var at = space(input, start)
  let ch = input.byte_at(at) ?? -1
  if ch >= 48 and ch <= 57 {
    let found = number(input, at)?
    if input.byte_at(found.next) == 126 {
      let step = number(input, found.next + 1)?
      return Ok({value: .Stride(found.value, step.value), next: step.next, previous: previous})
    }
    return Ok({value: .Line(found.value), next: found.next, previous: previous})
  }
  if ch == 36 { return Ok({value: .Last, next: at + 1, previous: previous}) }
  if ch == 43 and second {
    let found = number(input, at + 1)?
    return Ok({value: .Relative(found.value), next: found.next, previous: previous})
  }
  if ch == 47 or ch == 92 {
    at += 1
    let delimiter = if ch == 47 { 47 } else { input.byte_at(at) ?? -1 }
    if delimiter < 0 { return Err(SedError.Invalid("missing address delimiter")) }
    if ch == 92 { at += 1 }
    let found = delimited(input, at, delimiter, false)?
    at = found.next
    let insensitive = input.byte_at(at) == 73
    if insensitive { at += 1 }
    let compiled = make_regex(found.value, previous, extended, insensitive)?
    return Ok({value: .PatternAddress(compiled.value), next: at, previous: compiled.previous})
  }
  Ok({value: null, next: at, previous: previous})
}

pure tail(input: Bytes, start: Int) -> Result[BytesScan, Error] {
  let begin = space(input, start)
  var at = begin
  while at < input.len() and (input.byte_at(at) ?? -1) not in [59, 10, 125] { at += 1 }
  Ok({value: input[begin..at], next: at})
}

pure validate_replacement(replacement: Bytes, groups: Int) -> Result[Unit, Error] {
  var at = 0
  while at < replacement.len() {
    if replacement.byte_at(at) == 92 {
      at += 1
      let next = replacement.byte_at(at) ?? -1
      if next < 0 { return Err(SedError.Invalid("unterminated replacement escape")) }
      if next in [76, 85, 108, 117, 69] { return Err(SedError.Invalid("replacement case conversion is unsupported")) }
      if next >= 48 and next <= 57 and next - 48 > groups { return Err(SedError.Invalid("invalid replacement backreference")) }
    }
    at += 1
  }
  Ok()
}

pure primitive(op: Int) -> Primitive? {
  match op {
    100 => .Delete
    68 => .DeleteFirst
    112 => .Print
    80 => .PrintFirst
    104 => .Store
    72 => .AppendHold
    103 => .Load
    71 => .AppendPattern
    120 => .Exchange
    110 => .Next
    78 => .AppendNext
    113 => .Quit
    81 => .QuitQuiet
    61 => .Number
    122 => .Clear
    _ => null
  }
}

pure parse(script: Str, quiet: Bool, extended: Bool) -> Result[Program, Error] {
  let input = bytes.from_text(script)
  var at = 0
  var previous: RegexSpec? = null
  var commands: List[EditCommand] = []
  var labels: Map[Int] = {}
  var groups: List[Int] = []
  while at < input.len() {
    at = space(input, at)
    if at == input.len() { break }
    if (input.byte_at(at) ?? -1) in [59, 10] { at += 1; continue }
    if input.byte_at(at) == 35 {
      while at < input.len() and input.byte_at(at) != 10 { at += 1 }
      continue
    }
    let first_scan = address(input, at, false, previous, extended)?
    let first = first_scan.value
    previous = first_scan.previous
    at = space(input, first_scan.next)
    var last: Address? = null
    if input.byte_at(at) == 44 {
      let last_scan = address(input, at + 1, true, previous, extended)?
      if last_scan.value == null { return Err(SedError.Invalid("missing range end")) }
      last = last_scan.value
      previous = last_scan.previous
      at = last_scan.next
    }
    if first == .Line(0) {
      var regex_end = false
      if let value = last { match value { PatternAddress(_) => regex_end = true, _ => {} } }
      if !regex_end { return Err(SedError.Invalid("address zero requires a regular expression range end")) }
    }
    at = space(input, at)
    let invert = input.byte_at(at) == 33
    if invert { at = space(input, at + 1) }
    let op = input.byte_at(at) ?? -1
    if op < 0 { return Err(SedError.Invalid("missing command")) }
    # Append and insert act on one address; change commands can replace ranges.
    if op in [97, 105] and last != null { return Err(SedError.Invalid("append and insert commands use only one address")) }
    at += 1
    var action: Action = .End
    if op == 115 {
      let delimiter = input.byte_at(at) ?? -1
      if delimiter < 0 { return Err(SedError.Invalid("missing substitution delimiter")) }
      if delimiter in [10, 92] { return Err(SedError.Invalid("invalid substitution delimiter")) }
      let pattern = delimited(input, at + 1, delimiter, false)?
      let replacement = delimited(input, pattern.next, delimiter, true)?
      at = replacement.next
      var global = false
      var print_match = false
      var occurrence = 0
      var insensitive = false
      while at < input.len() {
        let flag = input.byte_at(at) ?? -1
        if flag == 103 and !global { global = true; at += 1 } else if flag == 112 and !print_match { print_match = true; at += 1 } else if flag in [73, 105] and !insensitive { insensitive = true; at += 1 } else if flag >= 48 and flag <= 57 and occurrence == 0 {
          let found = number(input, at)?
          occurrence = found.value
          at = found.next
          if occurrence == 0 { return Err(SedError.Invalid("substitution occurrence must be positive")) }
        } else if flag in [32, 9, 59, 10, 125] { break } else { return Err(SedError.Invalid(f"unsupported substitution flag '{input[at..at + 1].utf8()?}'")) }
      }
      let compiled = make_regex(pattern.value, previous, extended, insensitive)?
      previous = compiled.previous
      validate_replacement(replacement.value, compiled.previous.groups)?
      action = .Substitute({regex: compiled.value, replacement: replacement.value, global: global, occurrence: occurrence, print: print_match})
    } else if op == 121 {
      let delimiter = input.byte_at(at) ?? -1
      if delimiter < 0 { return Err(SedError.Invalid("missing transliteration delimiter")) }
      let from = delimited(input, at + 1, delimiter, false)?
      let to = delimited(input, from.next, delimiter, false)?
      at = to.next
      if from.value.len() != to.value.len() { return Err(SedError.Invalid("transliteration lists have different lengths")) }
      action = .Translate({from: from.value, to: to.value})
    } else if op in [97, 105, 99] {
      at = space(input, at)
      if input.byte_at(at) == 92 {
        at += 1
        if input.byte_at(at) == 10 { at += 1 }
      }
      var text: List[Bytes] = []
      while at < input.len() and input.byte_at(at) != 10 {
        let ch = input.byte_at(at) ?? -1
        at += 1
        if ch == 92 {
          if at == input.len() { return Err(SedError.Invalid("unterminated text escape")) }
          text += [if input.byte_at(at) == 110 { b"\n" } else { input[at..at + 1] }]
          at += 1
        } else { text += [input[at - 1..at]] }
      }
      let kind: TextKind = if op == 97 { .AppendText } else if op == 105 { .InsertText } else { .ChangeText }
      action = .Text({kind: kind, text: bytes.concat(text)})
    } else if op in [98, 116, 84, 58] {
      let found = tail(input, at)?
      at = found.next
      let label = found.value.utf8()?.trim()
      if op == 58 {
        if label == "" { return Err(SedError.Invalid("empty label")) }
        if label in labels { return Err(SedError.Invalid("duplicate label")) }
        labels = labels.set(label, commands.len())
        action = .Label(label)
      } else {
        let kind: BranchKind = if op == 98 { .Always } else if op == 116 { .OnSubstitution } else { .WithoutSubstitution }
        action = .Branch({kind: kind, label: label})
      }
    } else if op == 123 {
      groups += [commands.len()]
      action = .BlockEnd(0)
    } else if op == 125 {
      if first != null or invert { return Err(SedError.Invalid("closing brace cannot have an address")) }
      if groups.is_empty() { return Err(SedError.Invalid("unexpected closing brace")) }
      let begin = groups[groups.len() - 1]
      groups = groups[0..groups.len() - 1]
      commands[begin] = {...commands[begin], action: .BlockEnd(commands.len())}
    } else {
      guard let simple = primitive(op) else { return Err(SedError.Invalid(f"unsupported command '{input[at - 1..at].utf8()?}'")) }
      action = .Simple(simple)
    }
    commands += [{first: first, last: last, invert: invert, action: action}]
    at = space(input, at)
    if at < input.len() and (input.byte_at(at) ?? -1) not in [59, 10, 125, 35] and op != 123 { return Err(SedError.Invalid("extra characters after command")) }
  }
  if !groups.is_empty() { return Err(SedError.Invalid("unclosed command group")) }
  for command in commands {
    match command.action {
      Branch(branch) => {
        if branch.label != "" and branch.label not in labels { return Err(SedError.Invalid(f"undefined label: {branch.label}")) }
      }
      _ => {}
    }
  }
  Ok({commands: commands, labels: labels, extended: extended, quiet: quiet or script.starts_with("#n")})
}

pure resolve(choice: RegexChoice, previous: RegexSpec?) -> Result[RegexSpec, Error] {
  match choice {
    Explicit(spec) => Ok(spec)
    Previous => {
      guard let spec = previous else { return Err(SedError.Invalid("no previous regular expression")) }
      Ok(spec)
    }
  }
}

pure address_matches(address: Address, line: Int, total: Int, pattern: Bytes, start: Int, previous: RegexSpec?, extended: Bool) -> Result[AddressMatch, Error] {
  match address {
    Line(value) => Ok({matched: line == value, previous: previous})
    Stride(first, step) => {
      let start = if first == 0 and step > 0 { step } else { first }
      let matched = if step == 0 { line == start } else { line >= start and (line - start) % step == 0 }
      Ok({matched: matched, previous: previous})
    }
    Last => Ok({matched: line == total, previous: previous})
    Relative(value) => Ok({matched: line >= start + value, previous: previous})
    PatternAddress(choice) => {
      let spec = resolve(choice, previous)?
      let captures = regex.captures_bytes(spec.pattern, pattern, extended: extended, ignore_case: spec.insensitive)?
      Ok({matched: !captures.is_empty(), previous: spec})
    }
  }
}

# Range state belongs to each command and is updated before address inversion.
pure select(command: EditCommand, active: Int?, line: Int, total: Int, pattern: Bytes, previous: RegexSpec?, extended: Bool) -> Result[Selection, Error] {
  var saved = previous
  guard let first = command.first else { return Ok({selected: true, ending: false, active: active, previous: saved}) }
  guard let last = command.last else {
    let found = address_matches(first, line, total, pattern, 0, saved, extended)?
    return Ok({selected: found.matched, ending: false, active: active, previous: found.previous})
  }
  let already = active != null
  var begins = already or (first == .Line(0) and line == 1)
  if !begins {
    let found = address_matches(first, line, total, pattern, 0, saved, extended)?
    begins = found.matched
    saved = found.previous
  }
  if !begins { return Ok({selected: false, ending: false, active: active, previous: saved}) }
  let start = active ?? line
  var ending = false
  match last {
    Line(value) => ending = line >= value
    PatternAddress(_) => {
      if already or first == .Line(0) {
        let found = address_matches(last, line, total, pattern, start, saved, extended)?
        ending = found.matched
        saved = found.previous
      }
    }
    _ => {
      let found = address_matches(last, line, total, pattern, start, saved, extended)?
      ending = found.matched
      saved = found.previous
    }
  }
  Ok({selected: true, ending: ending, active: if ending { null } else { start }, previous: saved})
}

# Every print advances the output record boundary, even if its bytes are empty.
pure emit(output: Output, text: Bytes, newline: Bool) -> Output {
  var parts = output.parts
  if !parts.is_empty() and !output.separated { parts += [b"\n"] }
  parts += [text]
  if newline { parts += [b"\n"] }
  {parts: parts, separated: newline or text.byte_at(text.len() - 1) == 10}
}

pure replace(spec: RegexSpec, text: Bytes, substitution: Substitution, extended: Bool) -> Result[Replacement, Error] {
  var out: List[Bytes] = []
  var search = 0
  var copied = 0
  var count = 0
  var changed = false
  var previous_end: Int? = null
  while search <= text.len() {
    let captures: List[Capture?] = regex.captures_bytes(spec.pattern, text, offset: search, extended: extended, ignore_case: spec.insensitive)?
    if captures.is_empty() { break }
    guard let whole = captures[0] else { return Err(SedError.Invalid("missing whole-match capture")) }
    let begin = whole.start
    let end = whole.end
    if begin == end and previous_end == begin {
      if end == text.len() { break }
      search = end + 1
      continue
    }
    count += 1
    let selected = if substitution.occurrence == 0 { substitution.global or count == 1 } else { count == substitution.occurrence or (substitution.global and count > substitution.occurrence) }
    if selected {
      changed = true
      out += [text[copied..begin]]
      var at = 0
      let replacement = substitution.replacement
      while at < replacement.len() {
        let ch = replacement.byte_at(at) ?? -1
        at += 1
        if ch == 38 { out += [text[begin..end]] } else if ch == 92 {
          let next = replacement.byte_at(at) ?? -1
          if next < 0 { return Err(SedError.Invalid("unterminated replacement escape")) }
          at += 1
          if next >= 48 and next <= 57 {
            if let span = captures[next - 48] { out += [text[span.start..span.end]] }
          } else if let byte = control(next) { out += [bytes.from_ints([byte])?] } else { out += [replacement[at - 1..at]] }
        } else { out += [replacement[at - 1..at]] }
      }
      copied = end
      if !substitution.global { break }
    }
    previous_end = end
    if begin == end {
      if end == text.len() { break }
      search = end + 1
    } else { search = end }
  }
  out += [text[copied..]]
  Ok({text: bytes.concat(out), changed: changed})
}

pure records(inputs: List[Bytes]) -> List[EditRecord] {
  var result: List[EditRecord] = []
  for input in inputs {
    var start = 0
    for at in range(input.len()) {
      if input.byte_at(at) == 10 { result += [{text: input[start..at], newline: true}]; start = at + 1 }
    }
    if start < input.len() { result += [{text: input[start..], newline: false}] }
  }
  result
}

pure first_newline(text: Bytes) -> Int? {
  for at in range(text.len()) { return at when text.byte_at(at) == 10 }
  null
}

pure translate(text: Bytes, translation: Translation) -> Bytes {
  var out: List[Bytes] = []
  for at in range(text.len()) {
    var value = text[at..at + 1]
    for index in range(translation.from.len()) {
      if translation.from.byte_at(index) == text.byte_at(at) { value = translation.to[index..index + 1]; break }
    }
    out += [value]
  }
  bytes.concat(out)
}

pure execute(program: Program, inputs: List[Bytes]) -> Result[Bytes, Error] {
  let lines = records(inputs)
  var active: List[Int?] = [null for command in program.commands]
  var hold: EditRecord = {text: b"", newline: true}
  var output: Output = {parts: [], separated: true}
  var record = 0
  var previous: RegexSpec? = null
  while record < lines.len() {
    var pattern = lines[record]
    var pending: Output = {parts: [], separated: true}
    var pc = 0
    var deleted = false
    var substituted = false
    var quit = false
    while pc < program.commands.len() {
      let index = pc
      let command = program.commands[index]
      pc += 1
      let selection = select(command, active[index], record + 1, lines.len(), pattern.text, previous, program.extended)?
      active[index] = selection.active
      previous = selection.previous
      if selection.selected == command.invert {
        match command.action { BlockEnd(end) => pc = end + 1, _ => {} }
        continue
      }
      match command.action {
        Substitute(substitution) => {
          let spec = resolve(substitution.regex, previous)?
          previous = spec
          let replaced = replace(spec, pattern.text, substitution, program.extended)?
          pattern = {...pattern, text: replaced.text}
          substituted = substituted or replaced.changed
          if replaced.changed and substitution.print { output = emit(output, pattern.text, pattern.newline) }
        }
        Translate(translation) => pattern = {...pattern, text: translate(pattern.text, translation)}
        Text(text) => {
          if text.kind == .AppendText { pending = emit(pending, text.text, true) } else if text.kind == .InsertText { output = emit(output, text.text, true) } else {
            # A complemented range selects independent lines, each with its own replacement.
            if command.last == null or selection.ending or command.invert { output = emit(output, text.text, true) }
            deleted = true
            break
          }
        }
        Branch(branch) => {
          let take = branch.kind == .Always or (branch.kind == .OnSubstitution and substituted) or (branch.kind == .WithoutSubstitution and !substituted)
          if branch.kind != .Always { substituted = false }
          if take { pc = if branch.label == "" { program.commands.len() } else { program.labels[branch.label] + 1 } }
        }
        BlockEnd(_) => {}
        End => {}
        Label(_) => {}
        Simple(simple) => match simple {
          Delete => { deleted = true; break }
          DeleteFirst => {
            if let newline = first_newline(pattern.text) { pattern = {...pattern, text: pattern.text[newline + 1..]}; pc = 0 } else { deleted = true; break }
          }
          Print => output = emit(output, pattern.text, pattern.newline)
          PrintFirst => {
            let newline = first_newline(pattern.text)
            let first = pattern.text[0..newline ?? pattern.text.len()]
            output = emit(output, first, newline != null or pattern.newline)
          }
          Store => hold = pattern
          AppendHold => hold = {text: bytes.concat([hold.text, b"\n", pattern.text]), newline: pattern.newline}
          Load => pattern = hold
          AppendPattern => pattern = {text: bytes.concat([pattern.text, b"\n", hold.text]), newline: hold.newline}
          Exchange => { let saved = hold; hold = pattern; pattern = saved }
          Clear => pattern = {...pattern, text: b""}
          Number => output = emit(output, bytes.from_text(f"{record + 1}"), true)
          Quit => { quit = true; pattern = {...pattern, newline: true}; break }
          QuitQuiet => { quit = true; deleted = true; break }
          Next => {
            if !program.quiet { output = emit(output, pattern.text, pattern.newline) }
            if !pending.parts.is_empty() { output = emit(output, bytes.concat(pending.parts), false); pending = {parts: [], separated: true} }
            if record + 1 == lines.len() { deleted = true; quit = true; break }
            record += 1
            pattern = lines[record]
            substituted = false
          }
          AppendNext => {
            if record + 1 == lines.len() { quit = true; break }
            record += 1
            pattern = {text: bytes.concat([pattern.text, b"\n", lines[record].text]), newline: lines[record].newline}
            substituted = false
          }
        }
      }
    }
    if !deleted and !program.quiet { output = emit(output, pattern.text, pattern.newline) }
    if !pending.parts.is_empty() { output = emit(output, bytes.concat(pending.parts), false) }
    if quit { break }
    record += 1
  }
  Ok(bytes.concat(output.parts))
}

## Execute an editing program; report syntax or matching failures as sed diagnostics.
export proc edit(script: Str, inputs: List[Bytes], quiet: Bool, extended: Bool) [process] -> Bytes {
  let result = try { execute(parse(script, quiet, extended)?, inputs)? }
  match result {
    Ok(output) => output
    Err(failure) => { gnu.error(failure.message); exit 1 }
  }
}

## Read an editing program with sed's file-error status.
export proc read_script(name: Str) [fs, process, env] -> Str {
  match fp"{name}".read_text() {
    Ok(text) => text
    Err(failure) => { gnu.cannot_open(name, failure); exit 2 }
  }
}

## Read one byte operand, including standard input, with sed's file-error status.
export proc read_input(name: Str) [fs, error, io, process, env] -> Bytes {
  match gnu.read_operand(name) {
    Ok(data) => data
    Err(failure) => { gnu.cannot_open(name, failure); exit 2 }
  }
}

##! Size parsing and GNU upward rounding shared by df and du.
use gnu

## Display block size, human radix (zero disables scaling), and df heading.
export type Units = {block: Int, human: Int, label: Str}

## Round nonnegative counts upward to whole units.
export pure ceiling(value: Int, unit: Int) -> Int {
  let quotient = value / unit
  quotient + (if value % unit > 0 { 1 } else { 0 })
}

## Accept GNU size suffixes and the binary integer spelling supported by the
## reference parser, with checked multiplication instead of overflow clamping.
export pure parse_size(text: Str) -> Int? {
  let parts = rx"^(0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]*)([kKmMgGtTpPeEzZyYrRqQ]?)(i?B|D)?$".captures(text)
  return null when parts.is_empty() or text == ""
  let number = parts[1]
  return null when parts[2] == "" and parts[3] != ""
  var value = if number == "" { 1 } else { 0 }
  if number.lower().starts_with("0b") or number.lower().starts_with("0x") {
    let base = if number.lower().starts_with("0b") { 2 } else { 16 }
    for digit in number.byte_slice(2).lower() {
      let amount = "0123456789abcdef".find(digit) ?? -1
      return null when amount < 0 or value > (9223372036854775807 - amount) / base
      value = value * base + amount
    }
  } else if number.starts_with("0") and number.byte_len() > 1 {
    for digit in number {
      let amount = "01234567".find(digit) ?? -1
      return null when amount < 0 or value > (9223372036854775807 - amount) / 8
      value = value * 8 + amount
    }
  } else if number != "" {
    let parsed = number.parse_int()
    return null when parsed is Err(_)
    value = parsed ?? 0
  }
  let suffix = parts[2].upper()
  let base = if parts[3] in ["B", "D"] { 1000 } else { 1024 }
  if suffix != "" {
    let power = ("KMGTPEZYRQ".find(suffix) ?? -1) + 1
    return null when power <= 0
    for _ in range(power) {
      return null when value > 9223372036854775807 / base
      value *= base
    }
  }
  value
}

pure human_size(value: Int, base: Int) -> Str {
  return f"{value}" when value < base
  var unit = base
  var index = 0
  while unit <= 9223372036854775807 / base and value >= unit * base {
    unit *= base
    index += 1
  }
  let suffix = if base == 1000 and index == 0 { "k" } else { "KMGTPEZYRQ"[index..index + 1] }
  if value / unit < 10 {
    var fraction = 0
    let remainder = value % unit
    while fraction < 10 and remainder > unit / 10 * fraction + unit % 10 * fraction / 10 { fraction += 1 }
    let tenths = value / unit * 10 + fraction
    return if tenths >= 100 { f"{tenths / 10}{suffix}" } else { f"{tenths / 10}.{tenths % 10}{suffix}" }
  }
  f"{ceiling(value, unit)}{suffix}"
}

## Render an amount with upward rounding in the selected display units.
export pure amount(value: Int, units: Units) -> Str {
  if units.human != 0 { human_size(value, units.human) } else { f"{ceiling(value, units.block)}" }
}

## Apply utility-specific, generic, and POSIX environment precedence.
export proc environment_units(applet_name: Str, portable = false) [env] -> Units {
  let standard = if env.get("POSIXLY_CORRECT") is Ok(_) { 512 } else { 1024 }
  if ! portable {
    for key in [f"{applet_name.upper()}_BLOCK_SIZE", "BLOCK_SIZE", "BLOCKSIZE"] {
      if let Ok(text) = env.get(key) {
        if text == "human-readable" { return {block: 1, human: 1024, label: "Size"} }
        if text == "si" { return {block: 1, human: 1000, label: "Size"} }
        let size = parse_size(text)
        if size != null and size > 0 { return {block: size, human: 0, label: block_label(size)} }
        return {block: standard, human: 0, label: if standard == 1024 { "1K-blocks" } else { "512B-blocks" }}
      }
    }
  }
  {block: standard, human: 0, label: if standard == 1024 { "1K-blocks" } else { "512B-blocks" }}
}

## Prefer exact binary or decimal suffixes for df block-size headings.
export pure block_label(size: Int) -> Str {
  var decimal = size
  var binary = size
  var decimal_exact = false
  var binary_exact = false
  loop {
    decimal_exact = decimal % 1000 == 0
    binary_exact = binary % 1024 == 0
    decimal /= 1000
    binary /= 1024
    break when ! decimal_exact or ! binary_exact or decimal == 0 or binary == 0
  }
  let base = if binary_exact and ! decimal_exact { 1024 } else { 1000 }
  let value = human_size(size, base).replace(".0", with: "")
  value + (if base == 1000 { "B" } else { "" }) + "-blocks"
}

## Size selectors override earlier selectors in command order. Validate each
## explicit size even when a later selector replaces it.
export proc argument_units(argv: List[Str], initial: Units, value_flags: List[Str], si_short = "") [env, process, error] -> Units {
  var selected = initial
  let posix = env.get("POSIXLY_CORRECT") is Ok(_)
  var token_argv: List[Str] = []
  var options = true
  for raw in argv {
    var expanded = raw
    if options and raw.starts_with("--") and raw != "--" {
      let pieces = raw.byte_slice(2).split("=")
      let name = pieces[0]
      let matches = [flag for flag in value_flags if flag.byte_len() > 1 and flag.starts_with(name)]
      if matches.len() == 1 {
        expanded = "--" + matches[0] + (if pieces.len() > 1 { "=" + pieces[1..].join("=") } else { "" })
      }
    }
    token_argv += [expanded]
    if raw == "--" { options = false }
  }
  for token in cli.tokens(token_argv, value_flags)? {
    break when posix and token.kind == "operand"
    continue when token.kind == "operand"
    let name = token.name
    if (token.kind == "short" and name == "h") or (token.kind == "long" and "human-readable".starts_with(name)) {
      selected = {block: 1, human: 1024, label: "Size"}
    } else if (token.kind == "short" and name == si_short and si_short != "") or (token.kind == "long" and name == "si") {
      selected = {block: 1, human: 1000, label: "Size"}
    } else if (token.kind == "short" and name in ["k", "m", "b"]) or (token.kind == "long" and "bytes".starts_with(name)) {
      let size = if name == "k" { 1024 } else if name == "m" { 1048576 } else { 1 }
      selected = {block: size, human: 0, label: block_label(size)}
    } else if (token.kind == "short" and name == "B") or (token.kind == "long" and "block-size".starts_with(name)) {
      if token.value in ["human-readable", "si"] {
        selected = {block: 1, human: if token.value == "si" { 1000 } else { 1024 }, label: "Size"}
        continue
      }
      let size = parse_size(token.value)
      if size == null or size <= 0 {
        gnu.error(size_error(token.value, "block-size"))
        exit 1
      }
      selected = {block: size, human: 0, label: block_label(size)}
    }
  }
  selected
}

## Distinguish malformed numbers, unsupported suffixes, and integer overflow
## using the same lexical size contract as the parser.
export proc size_error(text: Str, option: Str) [env] -> Str {
  let syntax = rx"^(0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]*)([kKmMgGtTpPeEzZyYrRqQ]?)(i?B|D)?$".captures(text)
  var reason = "invalid"
  if ! syntax.is_empty() and text != "" {
    if syntax[2] == "" and syntax[3] != "" { reason = "invalid suffix in" } else if parse_size(text) == null { reason = "too large" }
  } else if rx"^[0-9]".matches(text) and text not in ["0b", "0x", "0X"] { reason = "invalid suffix in" }
  if reason == "too large" { f"--{option} argument {gnu.quote_value(text)} too large" } else { f"{reason} --{option} argument {gnu.quote_value(text)}" }
}

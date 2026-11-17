##! Matches libsmartcols byte escaping.

## Encodes a byte in libsmartcols hexadecimal notation.
export pure hex_byte(value: Int) -> Str {
  let digits = "0123456789abcdef"
  "\\x" + digits.byte_slice(value / 16, length: 1) + digits.byte_slice(value % 16, length: 1)
}

pure codepoint_value(value: Str) -> Int {
  let raw = bytes.from_text(value)
  let first = raw.byte_at(0) ?? 0
  return first when first < 128
  let second = (raw.byte_at(1) ?? 128) - 128
  return (first - 192) * 64 + second when raw.len() == 2
  let third = (raw.byte_at(2) ?? 128) - 128
  return (first - 224) * 4096 + second * 64 + third when raw.len() == 3
  let third_byte = (raw.byte_at(2) ?? 128) - 128
  let fourth_byte = (raw.byte_at(3) ?? 128) - 128
  (first - 240) * 262144 + second * 4096 + third_byte * 64 + fourth_byte
}

pure utf8_size(first: Int) -> Int {
  return 1 when first < 128
  return 2 when first >= 194 and first < 224
  return 3 when first >= 224 and first < 240
  return 4 when first >= 240 and first < 245
  0
}

## Applies libsmartcols table escaping with an optional newline exception.
export pure safe_table_text(raw: Bytes, allow_newline: Bool, byte_locale: Bool) -> Str {
  if byte_locale {
    var output = ""
    var at = 0
    while at < raw.len() {
      let byte = raw.byte_at(at) ?? 0
      if allow_newline and byte == 10 {
        output += "\n"
        at += 1
      } else if byte == 92 and at + 1 < raw.len() and raw.byte_at(at + 1) == 120 {
        output += hex_byte(byte)
        at += 1
      } else if byte >= 32 and byte < 127 {
        output += raw[at..at + 1].utf8() ?? ""
        at += 1
      } else {
        output += hex_byte(byte)
        at += 1
      }
    }
    return output
  }
  var output = ""
  var at = 0
  while at < raw.len() {
    let byte = raw.byte_at(at) ?? 0
    if byte == 92 and at + 1 < raw.len() and raw.byte_at(at + 1) == 120 {
      output += hex_byte(byte)
      at += 1
      continue
    }
    let size = utf8_size(byte)
    if size > 0 and at + size <= raw.len() {
      if let Ok(character) = raw[at..at + size].utf8() {
        let codepoint = codepoint_value(character)
        if allow_newline and codepoint == 10 {
          output += "\n"
        } else if codepoint <= 31 or codepoint in [127, 8232, 8233] or (codepoint >= 128 and codepoint < 160) or
            (codepoint > 8203 and codepoint < 8208) or (codepoint >= 8234 and codepoint < 8239) or
            (codepoint >= 8287 and codepoint < 8304) or codepoint == 65279 {
          for byte_index in range(size) { output += hex_byte(raw.byte_at(at + byte_index) ?? 0) }
        } else {
          output += character
        }
        at += size
        continue
      }
    }
    output += hex_byte(byte)
    at += 1
  }
  output
}

## Applies libsmartcols raw output byte escaping.
export pure raw_table_text(raw: Bytes, byte_locale: Bool) -> Bytes {
  var output: List[Bytes] = []
  for index in range(raw.len()) {
    let byte = raw.byte_at(index) ?? 0
    if byte == 32 or byte == 92 or byte < 32 or byte >= 127 {
      output += [bytes.from_text(hex_byte(byte))]
    } else {
      output += [raw[index..index + 1]]
    }
  }
  bytes.concat(output)
}


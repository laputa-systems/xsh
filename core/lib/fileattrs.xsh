##! Extended attribute value codecs and Linux file flag presentation.
use gnu

error AttributeError = Encoding : InvalidArgument | Path : InvalidArgument

type FileFlag = {letter: Str, bit: Int, name: Str, writable: Bool}

## Linux inode flags in the conventional display order.
export const FLAGS: List[FileFlag] = [
  {letter: "s", bit: 1, name: "Secure_Deletion", writable: true},
  {letter: "u", bit: 2, name: "Undelete", writable: true},
  {letter: "S", bit: 8, name: "Synchronous_Updates", writable: true},
  {letter: "D", bit: 65536, name: "Synchronous_Directory_Updates", writable: true},
  {letter: "i", bit: 16, name: "Immutable", writable: true},
  {letter: "a", bit: 32, name: "Append_Only", writable: true},
  {letter: "d", bit: 64, name: "No_Dump", writable: true},
  {letter: "A", bit: 128, name: "No_Atime", writable: true},
  {letter: "c", bit: 4, name: "Compression_Requested", writable: true},
  {letter: "E", bit: 2048, name: "Encrypted", writable: false},
  {letter: "j", bit: 16384, name: "Journaled_Data", writable: true},
  {letter: "I", bit: 4096, name: "Indexed_directory", writable: false},
  {letter: "t", bit: 32768, name: "No_Tailmerging", writable: true},
  {letter: "T", bit: 131072, name: "Top_of_Directory_Hierarchies", writable: true},
  {letter: "e", bit: 524288, name: "Extents", writable: true},
  {letter: "C", bit: 8388608, name: "No_COW", writable: true},
  {letter: "x", bit: 33554432, name: "DAX", writable: true},
  {letter: "F", bit: 1073741824, name: "Casefold", writable: true},
  {letter: "N", bit: 268435456, name: "Inline_Data", writable: false},
  {letter: "P", bit: 536870912, name: "Project_Hierarchy", writable: true},
  {letter: "V", bit: 1048576, name: "Verity", writable: false},
  {letter: "m", bit: 1024, name: "Dont_Compress", writable: true},
]

## Display raw inode flags, including flags that cannot be changed.
export pure flag_text(flags: Int, long = false) -> Str {
  var text = ""
  var names: List[Str] = []
  for flag in FLAGS {
    if flags.bit_and(flag.bit) != 0 { text += flag.letter; names += [flag.name] } else { text += "-" }
  }
  if ! long { text } else if names.is_empty() { "---" } else { names.join(", ") }
}

## Return a mutable inode flag bit, rejecting unknown or read-only letters.
export pure flag_bit(letter: Str) -> Int? {
  for flag in FLAGS { return flag.bit when flag.letter == letter and flag.writable }
  null
}

pure octal(byte: Int) -> Str { f"\\{byte / 64}{byte / 8 % 8}{byte % 8}" }

## Escape names so a line-oriented dump can be restored without changing them.
export pure quote_name(name: Str, attribute = false) -> Str {
  var text = ""
  for ch in name {
    if ch == "\n" { text += "\\012" } else if ch == "\r" { text += "\\015" } else if ch == "\\" { text += "\\134" } else if attribute and ch == "=" { text += "\\075" } else { text += ch }
  }
  text
}

## Encode an arbitrary xattr payload as text, hexadecimal, or base64.
export pure encode(value: Bytes, encoding: Str) -> Result[Bytes, Error] {
  return Err(AttributeError.Encoding("unknown value encoding")) when encoding not in ["", "text", "hex", "base64"]
  var format = encoding
  if format == "" {
    var nonprint = 0
    for index in range(value.len()) {
      let byte = value.byte_at(index) ?? 0
      if byte < 32 or byte > 126 { nonprint += 1 }
    }
    format = if value.len() >= nonprint * 8 { "text" } else { "base64" }
  }
  if format == "base64" { return bytes.from_text("0s" + value.base64()) }
  if format == "hex" {
    let digits = "0123456789abcdef"
    var text = "0x"
    for index in range(value.len()) {
      let byte = value.byte_at(index) ?? 0
      text += digits.byte_slice(byte / 16, length: 1) + digits.byte_slice(byte % 16, length: 1)
    }
    return bytes.from_text(text)
  }
  var chunks: List[Bytes] = [b"\""]
  for index in range(value.len()) {
    let byte = value.byte_at(index) ?? 0
    break when byte == 0 and index + 1 == value.len()
    if byte == 0 or byte == 10 or byte == 13 { chunks += [bytes.from_text(octal(byte))] } else if byte == 92 or byte == 34 { chunks += [b"\\", value[index..index + 1]] } else { chunks += [value[index..index + 1]] }
  }
  bytes.concat(chunks.extend([b"\""]))
}

## Decode backslash and octal escapes in a byte payload.
export pure unquote_bytes(raw: Bytes, quoted = false) -> Result[Bytes, Error] {
  var text = raw
  if quoted and text.len() >= 2 and text.byte_at(0) == 34 and text.byte_at(text.len() - 1) == 34 { text = text[1..text.len() - 1] }
  var decoded: List[Int] = []
  var at = 0
  while at < text.len() {
    let byte = text.byte_at(at) ?? 0
    at += 1
    if byte != 92 or at == text.len() { decoded += [byte]; continue }
    let next = text.byte_at(at) ?? 0
    if next == 92 or next == 34 { decoded += [next]; at += 1; continue }
    if next >= 48 and next <= 55 {
      var value = 0
      var count = 0
      while at < text.len() and count < 3 {
        let digit = text.byte_at(at) ?? 0
        break when digit < 48 or digit > 55
        value = value * 8 + digit - 48
        count += 1
        at += 1
      }
      decoded += [value % 256]
    } else { decoded += [byte] }
  }
  bytes.from_ints(decoded)
}

## Decode escaped text from an argument or a dump name.
export pure unquote(raw: Str, quoted = false) -> Result[Bytes, Error] {
  unquote_bytes(bytes.from_text(raw), quoted: quoted)
}

## Decode setfattr's explicit encodings; --raw bypasses this conversion.
export pure decode(raw: Str) -> Result[Bytes, Error] {
  if raw.starts_with("0x") or raw.starts_with("0X") {
    let text = rx"[[:space:]]".replace(raw.byte_slice(2), with: "")
    return Err(AttributeError.Encoding("bad input encoding")) when text.byte_len() % 2 != 0 or ! rx"^[0-9A-Fa-f]*$".matches(text)
    var values: List[Int] = []
    for index in range(text.byte_len() / 2) { values += [("0x" + text.byte_slice(index * 2, length: 2)).parse_int()?] }
    return bytes.from_ints(values)
  }
  if raw.starts_with("0s") or raw.starts_with("0S") {
    return rx"[[:space:]]".replace(raw.byte_slice(2), with: "").base64_decode()
  }
  unquote(raw, quoted: true)
}

## Convert an escaped dump name back to a path or attribute name.
export pure unquote_name(raw: Str) -> Result[Str, Error] {
  let decoded = unquote(raw)?
  decoded.utf8()
}

## Preserve per-operand errors and the conventional missing-attribute message.
export proc report(name: Str, failure: Error, attribute: Str = "") [process] {
  let message = if failure.errno == 61 or failure.errno == 93 { "No such attribute" } else { gnu.strerror(failure) }
  if attribute != "" { eprint f"{quote_name(name)}: {quote_name(attribute, attribute: true)}: {message}" } else { eprint f"{gnu.prog()}: {quote_name(name)}: {message}" }
}

## Pad a display field to a minimum byte width.
export pure field(value: Str, width: Int, left = true) -> Str {
  var text = value
  while text.byte_len() < width { text = if left { text + " " } else { " " + text } }
  text
}

## Decode a dump value without requiring its raw text bytes to be UTF-8.
export pure decode_bytes(raw: Bytes) -> Result[Bytes, Error] {
  if raw.len() >= 2 and raw[0..2] in [b"0x", b"0X", b"0s", b"0S"] {
    return decode(raw.utf8()?)
  }
  unquote_bytes(raw, quoted: true)
}

##! Image helpers.
const case_dir = "guest/asterinas/device"

## Parses a byte size from text.
export pure parse_size_bytes(text: Str) -> Int {
  return text.byte_len()
}

## Formats a byte size.
export pure format_size(size: Int) -> Str {
  return f"{size}B"
}

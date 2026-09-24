##! Embedded JSON Lines implementation.
# The codec and path operations live in `src/modules/json.rs`. This module
# keeps the bulk JSON Lines encoder; the 30-sample B0 workload measures it far
# below the older per-item native boundary.

# Runtime bridge used only to report a dynamic argument's original type.
export pure type_name(value: Any) -> Str {
  return [value][1]
}

error JsonError = Lines(kind: Str, message: Str)

pure lines_error(message: Str) -> JsonError {
  return JsonError.Lines(kind: "type-error", message: message)
}

## Encode values as JSON Lines text.
##
## Every item is encoded compactly on its own line, one newline per item, so an
## empty list produces the empty string. The whole text is built before it is
## returned, so an item that cannot be encoded reports its own failure and no
## partial text is produced.
export pure encode_lines(values: List[Any]) -> Result[Str] {
  match values {
    items is List[Any] => return Ok([(json.encode(item)?) + "\n" for item in items].join(""))
    _ => return Err(lines_error(f"expected List, found ${type_name(values)}"))
  }
}

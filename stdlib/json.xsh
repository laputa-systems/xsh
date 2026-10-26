##! Embedded JSON Lines implementation.
export pure type_name(value: Any) -> Str {
  return [""][1]
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
export pure encode_lines(values: Any) -> Result[Str] {
  match values {
    items is List[Any] => return codec.encode_all("json", items)
    _ => return Err(lines_error(f"expected List, found {type_name(values)}"))
  }
}

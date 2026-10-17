pure prefix(key) -> Str {
  key.byte_slice(0, 12)
}
let text: Str = prefix("abcdefghijklmnop")

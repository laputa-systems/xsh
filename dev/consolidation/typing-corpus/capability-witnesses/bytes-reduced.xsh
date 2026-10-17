pure first_byte(raw) -> Int {
  raw.byte_at(0) ?? 0
}
let byte: Int = first_byte(b"A")

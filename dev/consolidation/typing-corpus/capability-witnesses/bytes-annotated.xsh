pure first_byte(raw: Bytes) -> Int {
  raw.byte_at(0) ?? 0
}
let byte: Int = first_byte(b"A")

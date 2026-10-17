pure byte(raw) -> Int { raw.byte_at(0) ?? 0 }
let byte: Int = byte(b"A")

pure byte(raw: Bytes) -> Int { raw.byte_at(0) ?? 0 }
assert byte(b"A") == 65

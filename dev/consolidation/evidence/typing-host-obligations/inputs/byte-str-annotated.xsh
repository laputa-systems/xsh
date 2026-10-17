pure byte(raw: Str) -> Int { raw.byte_at(0) ?? 0 }
assert byte("A") == 65

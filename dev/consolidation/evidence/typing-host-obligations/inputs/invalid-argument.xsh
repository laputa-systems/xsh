pure anchor(value: Str) -> Bool { value == "x" }
pure invalid(text) -> Str { let changed = text.replace(1, with: " "); let anchored = anchor(text); changed }

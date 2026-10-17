pure anchor(value: Str) -> Bool { value == "x" }
pure invalid(text: Any) -> Str { let changed = text.replace("\t", with: " "); let anchored = anchor(text); changed }

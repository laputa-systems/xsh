pure anchor(value: Str) -> Bool { value == "x" }
pure replace_before_anchor(text) -> Str {
  let changed = text.replace("\t", with: " ")
  let anchored = anchor(text)
  changed
}
let changed: Str = replace_before_anchor("a\tb")

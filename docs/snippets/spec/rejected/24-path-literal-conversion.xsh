pure leaf(target: Path) -> Str {
  target.name()
}

proc unused(text: Str, target: Path) {
  let bound: Path = text # error: check.type-mismatch
  let built: Path = f"{text}/x" # error: check.type-mismatch
  print (leaf(text)) # error: check.type-mismatch
  print (target == text) # error: check.type-mismatch
  let nul: Path = "a\0b" # error: check.type-mismatch
  print $bound $built $nul
}

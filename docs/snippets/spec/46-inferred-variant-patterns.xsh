# begin example
enum Kind { File, Binary, Tree(Int) }

error ProofError {
    Usage
    Missing(file: Path)
}

pure classify(file: Path) -> Result[Kind, ProofError] {
  fail .Usage("no file named") when file == p""
  fail .Missing(file:) when file == p"gone"
  return Ok(.Tree(2)) when file == p"usr"
  Ok(.Binary)
}

pure describe(file: Path) -> Str {
  match classify(file) {
    Ok(.Tree(depth)) => f"a tree {depth} deep"
    Ok(.File | .Binary) => "a leaf"
    Err(.Usage {message}) => f"usage: {message}"
    Err(.Missing {file: missing}) => f"{missing} is missing"
    else => "unknown"
  }
}

let kind = classify(p"usr/bin/xsh") ?? .File
print describe(p"usr")
print describe(p"gone")
print describe(p"")
print (kind is .Binary)
# end example

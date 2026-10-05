pure fields(line: Str) -> Str {
  # begin example
  match line {
    f"{name}{rest}" => name + rest # error: check.text-pattern
    f"{count:05d} items" => f"{count}" # error: check.text-pattern
    else => line
  }
  # end example
}

print f"{fields("3 items")}"

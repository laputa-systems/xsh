use raw-ir-features-provider as provider

pure label(line: Str) -> Str {
  match line {
    f"key={value}" => value
    "on" | "yes" => "set"
    else => "other"
  }
}

for line in ["key=a", "on", "off"] |> where { |line| line != "" } {
  print f"{provider.marker()}{label(line)}"
}

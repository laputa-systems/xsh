# begin example
let described = try run.text sh -c "exit 3"
let label = if let Ok(text) = described { text.trim() } else { "unknown" }
# end example

print $label

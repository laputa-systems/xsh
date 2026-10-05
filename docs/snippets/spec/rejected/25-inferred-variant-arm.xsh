enum Kind { File, Binary }

# begin example
let kind: Kind = .Binary
let label = match kind {
  File => "file"
  .Binary => "binary"  # error: parse.inferred-variant-arm
}
# end example

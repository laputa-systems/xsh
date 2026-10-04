enum Kind { File, Binary }

error ProofError = Failed(message: Str)

# begin example
let kind = .Binary                         # error: check.inferred-variant

proc check(present: Bool) -> Result[Unit] {
  return Err(.Failed("missing")) when ! present  # error: check.inferred-variant
}

let tools = [{name: "xsh", kind: Binary}] |> where .kind == .Binary  # error: check.unknown-field
# end example

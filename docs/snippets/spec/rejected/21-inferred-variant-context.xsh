enum Kind { File, Binary }

error ProofError = Failed(message: Str)

# begin example
let kind = .Binary                         # error: check.inferred-variant

proc check(present: Bool) -> Result[Unit] {
  return Err(.Failed("missing")) when ! present  # error: check.inferred-variant
}

match check(false) {
  Err(.Failed {message}) => print $message  # error: check.inferred-variant
  Err(ProofError.Failed {message}) => print $message
  _ => print "present"
}

let tools = [{name: "xsh", kind: Binary}] |> where .kind == .Binary  # error: check.unknown-field
# end example

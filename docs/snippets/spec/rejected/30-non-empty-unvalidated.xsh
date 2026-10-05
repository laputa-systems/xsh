pure program(argv: NonEmpty[Str]) -> Str {
  argv.first()
}

let extra: List[Str] = ["status"]
let none: NonEmpty[Str] = [] # error: check.validated-literal
let maybe: NonEmpty[Str] = [@extra] # error: check.validated-literal
let argv: NonEmpty[Str] = ["git", @extra]
let rest: NonEmpty[Str] = argv[1..] # error: check.type-mismatch
print ${program(extra)} # error: check.type-mismatch
print ${extra.first()} # error: check.unknown-method
print ${none.len()} ${maybe.len()} ${rest.len()}

proc descriptor() [] -> Record { {count: {kind: "Int", default: 2, deprecated: "use jobs"}} }
type ParsedValues = {count: Int}
let full = cli.parse_full(["--count", "4"], descriptor())?
let values = full.values.require(ParsedValues)?
let warnings: List[Str] = full.warnings
let source = full.sources.get("count")?.require(Str)?
print ${values.count} $source warnings.len()

type IndexedSources = {name: Str}

proc main() [error] -> Result[Unit] {
  let exports: Record = {sources: {name: "demo"}}
  let sources = exports.get("sources")?.require(IndexedSources)?

  if sources.keys().len() != 0 {
    print "non-empty"
  }

  return Ok()
}

proc main() [error] -> Result[Unit] {
  let exports: Record = {sources: {name: "demo"}}
  let sources = exports.get("sources")?

  if sources.len() != 0 {
    print "non-empty"
  }

  return Ok()
}

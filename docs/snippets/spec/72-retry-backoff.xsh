proc fetch_index() [process, error] -> Result[Str] {
  run.text --timeout=10s curl -fsS https://example.com/index.json
}

# begin example
let index = retry backoff 100ms..5s within 30s on (is Timeout) {
  fetch_index()?
}?
# end example

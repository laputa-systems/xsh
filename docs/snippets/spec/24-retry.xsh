proc fetch_index() [process, error] -> Result[Str] {
  run.text --timeout=10s curl -fsS https://example.com/index.json
}

# begin example
let index = retry [1s, 2s, 4s] on (is Timeout) {
  fetch_index()?
}?
# end example

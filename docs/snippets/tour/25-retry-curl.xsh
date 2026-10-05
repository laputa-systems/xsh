const url = "https://mirror.example.org/releases/index.json"
let body = retry [1s, 2s, 4s] {
  run.text --timeout=10s curl -fsS $url
}

match body {
  Ok(text) => print f"fetched {text.byte_len()} bytes"
  Err(error) => print f"mirror unavailable: {error.message}"
}

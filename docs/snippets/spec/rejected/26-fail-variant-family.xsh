error FetchError = RemoteFetch | Offline

proc fetch(url: Str) -> Result[Str] {
  fail .Offline() when url == "" # error: check.inferred-variant
  Ok("body")
}

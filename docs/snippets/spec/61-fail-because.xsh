error FetchError = RemoteFetch | Offline

proc download(url: Str) -> Result[Str] {
  fail f"no route to {url}" unless url.starts_with("https:")
  Ok("body")
}

proc fetch(url: Str) -> Result[Str, FetchError] {
  fail .Offline() when url == ""
  match download(url) {
    Ok(body) => Ok(body)
    # begin example
    Err(problem) => return Err(.RemoteFetch(f"fetching {url}"), cause: problem)
    # end example
  }
}

match fetch("ftp://mirror/index") {
  Ok(body) => print $body
  Err(FetchError.RemoteFetch {message}) => print $message
  Err(FetchError.Offline) => print "offline"
}

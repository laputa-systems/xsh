proc fetch(url: Str, limit: Duration) [process, time, error] -> Result[Str] {
  # begin example
  let page = within limit {
    run.text curl --silent $url ?
  }
  match page {
    Ok(text) => text
    Err(is Timeout) => fail f"{url} did not answer in time"
    Err(failure) => Err(failure)
  }
  # end example
}

let quick = within 5s {
  time.sleep(1ms)
  "done"
}
print f"{quick ?? "late"}"

proc fetch(url: Str) [net, error] -> Result[Int] {
  net.request({method: "GET", url: url})?.status
}

proc refresh(url: Str, cache: Path) [fs, net, error] {
  without net {
    let status = fetch(url)? # error: check.effect-violation
    cache.write(f"{status}")?
  }
}

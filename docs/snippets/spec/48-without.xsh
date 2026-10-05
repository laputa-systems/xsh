proc fetch_sources(index: Str, staged: Path) [fs, net, error] {
  let response = net.request({method: "GET", url: index})?
  fp"{staged}/index.status".write(f"{response.status}")
}

proc build_from_staged_sources(staged: Path) [fs, process, error] {
  fp"{staged}/out".mkdir()
  run make -C $staged
}

# begin example
proc build(index: Str, staged: Path) [fs, net, process, error] {
  fetch_sources(index, staged)

  # Everything after the fetch is offline, and the checker holds it to that.
  without net {
    build_from_staged_sources(staged)
  }
}
# end example

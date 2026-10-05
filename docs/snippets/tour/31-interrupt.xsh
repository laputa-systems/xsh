proc fetch(url: Str, out: Path) {
  let partial = fp"{out}.partial"
  defer {
    partial.remove()
    print "removed partial download"
  }

  run curl -fsSL -o $partial $url
  partial.rename(to: out)
}

fetch("https://mirror.example.org/laputa.iso", p"laputa.iso")

proc fetch(url: Str, out: Path) {
  let partial = fp"{out}.partial"
  defer {
    fs.remove(partial, missing_ok: true)?
    print "removed partial download"
  }

  run curl -fsSL -o $partial $url
  fs.rename(partial, out)?
}

fetch("https://mirror.example.org/laputa.iso", p"laputa.iso")?

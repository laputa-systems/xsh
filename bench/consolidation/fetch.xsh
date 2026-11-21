use stdlib.source_locks as locks

cli main(lock: Path, destination: Path, arch: Str) {
  guard arch in ["amd64", "arm64"] else { fail "unsupported architecture" }
  let target = if arch == "amd64" { "x86_64-unknown-linux-musl" } else { "aarch64-unknown-linux-musl" }
  let sources = locks.load_sources(lock)?
  var checksums: List[Str] = []
  for item in sources {
    if item.target != null and item.target != target { continue }
    let name = if item.name == f"nushell-{arch}" { "nu" } else if item.name == f"elvish-{arch}" { "elvish" } else { item.name }
    let filename = f"{name}.tar.gz"
    let _ = locks.fetch_input(item, fp"{destination}/{filename}")?
    checksums = [@checksums, f"{item.sha256}  {filename}\n"]
  }
  fp"{destination}/checksums".write(checksums.join(""))?
}

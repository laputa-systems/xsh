##! Source lock validation, hashing and verified downloads.

## A validated source pin; additional JSON fields are retained in `data`.
export type Input = {name: Str, version: Str, sha256: Str, urls: List[Str], size: Int?, role: Str?, target: Str?, license: Str?, purpose: Str?, data: Map[Str, Any]}

## Return whether text is a lowercase SHA-256 digest.
export pure is_sha256(text: Str) -> Bool {
  rx"^[0-9a-f]{64}$".matches(text)
}

pure text(entry: Map[Str, Any], key: Str) -> Result[Str] {
  let value = entry.get(key)?.require(Str)?
  guard value != "" else { return Err(error.failure(f"{key} must be a nonempty string")) }
  return Ok(value)
}

pure optional_text(entry: Map[Str, Any], key: Str) -> Result[Str?] {
  if key in entry { return Ok(text(entry, key)?) }
  return Ok(null)
}

pure valid_url(url: Str) -> Bool {
  if !url.starts_with("https://") { return false }
  let authority = url.byte_slice(8).split("/")[0].split("?")[0].split("#")[0]
  let hostport = authority.split("@")[-1]
  var host = hostport
  var port = ""
  if hostport.starts_with("[") {
    let parts = hostport.split("]")
    if parts.len() != 2 or (parts[1] != "" and !parts[1].starts_with(":")) { return false }
    host = parts[0].byte_slice(1)
    port = if parts[1] == "" { "" } else { parts[1].byte_slice(1) }
  } else {
    let parts = hostport.split(":")
    if parts.len() > 2 { return false }
    host = parts[0]
    if parts.len() == 2 { port = parts[1] }
  }
  if host == "" { return false }
  if port == "" { return true }
  rx"^[0-9]{1,5}$".matches(port) and (port.parse_int() ?? 65536) <= 65535
}

## Read a source lock. JSON integers, including sizes and extras, must fit XSH's signed 64-bit Int.
export proc load_sources(file: Path) -> Result[List[Input], Error] {
  let document = json.read(file)?.require(Map[Str, Any])?
  guard document.get("schema_version")?.require(Int)? == 1 else { return Err(error.failure(f"{file}: schema_version must be 1")) }
  let raw = document.get("inputs")?.require(List[Any])?
  var entries: List[Input] = []
  var names: List[Str] = []
  for value in raw {
    let entry = value.require(Map[Str, Any])?
    let name = text(entry, "name")?
    guard name not in names else { return Err(error.failure(f"{file}: duplicate input {name}")) }
    names = [@names, name]
    let version = text(entry, "version")?
    let sha256 = text(entry, "sha256")?
    guard is_sha256(sha256) else { return Err(error.failure(f"{file}: {name}: invalid sha256")) }
    guard ("url" in entry) != ("urls" in entry) else { return Err(error.failure(f"{file}: {name}: exactly one of url and urls is required")) }
    let urls = if "url" in entry { [entry.get("url")?.require(Str)?] } else { entry.get("urls")?.require(List[Str])? }
    guard !urls.is_empty() else { return Err(error.failure(f"{file}: {name}: urls must not be empty")) }
    var seen: List[Str] = []
    for url in urls {
      guard valid_url(url) else { return Err(error.failure(f"{file}: {name}: invalid URL {url}")) }
      guard url not in seen else { return Err(error.failure(f"{file}: {name}: repeated URL")) }
      seen = [@seen, url]
    }
    let size: Int? = if "size" in entry { entry.get("size")?.require(Int)? } else { null }
    if let length = size { guard length >= 0 else { return Err(error.failure(f"{file}: {name}: invalid size")) } }
    let role = optional_text(entry, "role")?
    let target = optional_text(entry, "target")?
    let license = optional_text(entry, "license")?
    let purpose = optional_text(entry, "purpose")?
    entries = [@entries, {name: name, version: version, sha256: sha256, urls: urls, size: size, role: role, target: target, license: license, purpose: purpose, data: entry}]
  }
  return Ok(entries)
}

## A file digest and byte count.
export type FileDigest = {sha256: Str, size: Int}

## Return a file's lowercase SHA-256 and size.
export proc digest_file(file: Path) -> Result[FileDigest, Error] {
  let size = fs.stat(file)?.size
  let sha256 = hash.sha256(file)?.hex()
  return Ok({sha256: sha256, size: size})
}

## Return a file's lowercase SHA-256.
export proc sha256_file(file: Path) -> Result[Str, Error] {
  return Ok(hash.sha256(file)?.hex())
}

## Verify a regular file against its source pin.
export proc verify_file(file: Path, item: Input) -> Result[Unit, Error] {
  let info = fs.stat(file)?
  guard info.kind == "file" else { return Err(error.failure(f"{item.name}: not a regular file")) }
  if let size = item.size { guard info.size == size else { return Err(error.failure(f"{item.name}: size mismatch")) } }
  guard hash.sha256(file)?.hex() == item.sha256 else { return Err(error.failure(f"{item.name}: sha256 mismatch")) }
  return Ok()
}

proc download(item: Input, url: Str, destination: Path) -> Result[Unit] {
  let staged = fs.temp_sibling(destination)?
  defer staged.remove(missing_ok: true)
  if let size = item.size {
    let _ = net.download({url: url, dest: staged, timeout: 120s, redirects: 10, fail_status: true, max_body_bytes: size, headers: [{name: "User-Agent", value: "laputa-buildlib/0.1"}]})?
  } else {
    let _ = net.download({url: url, dest: staged, timeout: 120s, redirects: 10, fail_status: true, headers: [{name: "User-Agent", value: "laputa-buildlib/0.1"}]})?
  }
  verify_file(staged, item)?
  staged.rename(to: destination, overwrite: true)?
  return Ok()
}

## Fetch an input and publish it only after verification.
export proc fetch_input(item: Input, destination: Path) -> Result[Path, Error] {
  if verify_file(destination, item) is Ok(_) { return Ok(destination) }
  if destination.is_symlink() ?? false { return Err(error.failure(f"{item.name}: refusing to replace symlink")) }
  var failures: List[Str] = []
  for url in item.urls {
    match download(item, url, destination) {
      Ok(_) => return Ok(destination)
      Err(failure) => failures = [@failures, f"{url}: {failure.message}"]
    }
  }
  return Err(error.failure(f"{item.name}: no URL served the pinned bytes: {failures.join("; ")}"))
}

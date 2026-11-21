type Rotated = {log: Str, bytes: Int, deleted: List[Str]}

type Generation = {k: Int, path: Path}

const generation_digits = rx"^[1-9][0-9]*$"

pure generation_number(name: Str, log: Str) -> Int? {
  let prefix = f"{log}."
  if ! name.starts_with(prefix) or ! name.ends_with(".gz") or name.byte_len() <= prefix.byte_len() + 3 {
    return null
  }

  let digits = name.byte_slice(prefix.byte_len(), name.byte_len() - prefix.byte_len() - 3)
  if ! generation_digits.matches(digits) {
    return null
  }

  if let Ok(k) = digits.parse_int() { k } else { null }
}

proc rotate(dir: Path, log: Str, size: Int, keep: Int, names: List[Str]) -> Result[Rotated] {
  var generations: List[Generation] = []
  for name in names {
    let k = generation_number(name, log)
    if k != null {
      generations += [Generation(k:, path: fp"{dir}/{name}")]
    }
  }

  var deleted: List[Str] = []
  for generation in generations |> sort-by(desc: true) .k {
    if generation.k + 1 <= keep {
      generation.path.rename(to: fp"{dir}/{log}.{generation.k + 1}.gz")?
    } else {
      generation.path.remove()?
      deleted = [f"{log}.{generation.k}.gz"] + deleted
    }
  }

  let source = fp"{dir}/{log}"
  archive.compress(source, fp"{dir}/{log}.1.gz", .Gzip)?
  source.truncate(0)?
  Rotated(log:, bytes: size, deleted:)
}

## Rotate the logs in DIR, keeping --keep compressed generations.
cli main(dir: Path, keep: Int = 3, min_bytes: Int = 1024) {
  if min_bytes < 0 {
    eprint "usage: rotate [--keep N] [--min-bytes N] DIR (--keep at least 1, --min-bytes at least 0)"
    exit 2
  }

  let entries = fs.children(dir, stat: true)? |> collect()
  let names = [e.name for e in entries]
  let logs = entries
    |> where { |e| e.kind == "file" and e.name.ends_with(".log") and e.size >= min_bytes and e.size >= 1 }
    |> sort-by .name

  for entry in logs {
    let rotated = rotate(dir, entry.name, entry.size, keep, names)?
    print (json.encode(rotated)?)
  }
}

let names = [line.trim() for line in p"/work/bundle.txt".lines()? if ! line.trim().is_empty()]

for name in names {
  if ! fp"/work/logs/{name}".exists()? {
    eprint f"missing log file: {name}"
    exit 4
  }
}

let entries = [fp"logs/{name}" for name in names]
archive.create(p"/work/out/logs.tar.gz", p"/work", entries, .Tar, collision: .Replace)
print names.len()

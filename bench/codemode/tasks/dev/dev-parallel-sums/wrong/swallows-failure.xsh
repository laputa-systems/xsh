let files = [line.trim() for line in p"/work/files.txt".lines()? if ! line.trim().is_empty()]

let sums = files |> par-map(jobs: 3) { |file|
  let sum = try run.text /work/bin/slowsum $file
  {file: file, sha256: (sum ?? "").trim()}
}

for row in sums {
  print (json.encode(row)?)
}

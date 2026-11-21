type Row = {ok: Bool, line: Str}

let files = [line.trim() for line in p"/work/files.txt".lines()? if ! line.trim().is_empty()]

let rows = files |> par-map(jobs: 3) { |file|
  let out = run.capture --text /work/bin/slowsum $file
  if out.status.ok {
    Row(ok: true, line: json.encode({file: file, sha256: out.stdout.trim()})?)
  } else {
    let message = (out.stderr.lines().get(0) ?? "").trim()
    Row(ok: false, line: json.encode({file: file, error: message})?)
  }
}

for row in rows {
  print $row.line
}

exit 4 unless rows |> all { .ok }

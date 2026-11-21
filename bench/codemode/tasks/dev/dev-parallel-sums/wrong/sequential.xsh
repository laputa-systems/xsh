type Row = {ok: Bool, line: Str}

let files = [line.trim() for line in p"/work/files.txt".lines()? if ! line.trim().is_empty()]
var rows: List[Row] = []
for file in files {
  let out = run.capture --text /work/bin/slowsum $file
  if out.status.ok {
    rows += [Row(ok: true, line: json.encode({file: file, sha256: out.stdout.trim()})?)]
  } else {
    rows += [Row(ok: false, line: json.encode({file: file, error: out.stderr.trim()})?)]
  }
}

for row in rows {
  print $row.line
}

exit 4 unless rows |> all { .ok }

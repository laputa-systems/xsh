const access_line = rx"""^(\S+) \S+ \S+ \[[^\]]+\] "(\S+) (\S+) [^"]*" (\d{3}) (\d+)"""

type Hit = {client: Str, method: Str, url: Str, status: Int, size: Int}

stream hits(file: Path) [fs, error] -> Stream[Hit] {
  for line in file.lines()? {
    let fields = access_line.captures(line)
    if fields.len() == 6 {
      yield Hit(
        client: fields[1],
        method: fields[2],
        url: fields[3],
        status: fields[4].parse_int()?,
        size: fields[5].parse_int()?,
      )
    }
  }
}

const sample = """
  10.0.0.5 - - [03/Oct/2026:10:00:01 +0000] "GET /api/users HTTP/1.1" 200 512
  10.0.0.7 - - [03/Oct/2026:10:00:02 +0000] "GET /api/orders HTTP/1.1" 500 31
  10.0.0.5 - - [03/Oct/2026:10:00:03 +0000] "POST /api/orders HTTP/1.1" 500 31
  10.0.0.9 - - [03/Oct/2026:10:00:04 +0000] "GET /healthz HTTP/1.1" 200 500
  a line that is not an access log entry
  10.0.0.7 - - [03/Oct/2026:10:00:05 +0000] "GET /api/users HTTP/1.1" 503 0
  """

let scratch = fs.tempdir()?
defer scratch.close()?
let log = fp"${scratch.host_path()?}/access.log"
log.write(sample)?

let failures = hits(log)
  |> where .status >= 500
  |> count { |hit| hit.url }

for {key, value} in failures {
  print f"${value} ${key}"
}

let heaviest = hits(log) |> sort-by(desc: true) .size |> take(2) |> map .client
print f"heaviest clients: ${heaviest.join(" ")}"

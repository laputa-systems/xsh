let scratch = fs.tempdir()?
defer scratch.close()?
let root = scratch.host_path()?

for file in [
  "etc/hosts.conf",
  "etc/app.conf",
  "var/log/app.log",
  "var/log/app.log.gz",
  "srv/www/index.html",
  "srv/www/app.js",
  "srv/www/style.css",
  "srv/README",
] {
  let target = fp"${root}/${file}"
  target.parent().mkdir()?
  target.write("x\n")?
}

let per_dir = fs.children(root)?
  |> where .kind == "dir"
  |> par-map(jobs: 4) { |dir|
    {dir: dir.name, counts: fs.files(dir.path)? |> count { |f| f.ext }}
  }

let totals = per_dir |> fold(map.empty()) { |acc, part|
  var merged = acc
  for {key, value} in part.counts {
    merged[key] = (merged.get(key) ?? 0) + value
  }

  merged
}

for {key, value} in totals {
  let ext = if key == "" { "(none)" } else { key }
  print f"${value:>3} ${ext}"
}

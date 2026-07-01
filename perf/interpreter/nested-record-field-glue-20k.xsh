type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}

type Scan = {stats: Stats, deep: Stats}

pure make_scan(i: Int) -> Scan {
  let stats = {blanks: i % 3, code: i % 5, comments: i % 7, blobs: map.empty()}
  let deep = {blanks: i % 11, code: i % 13, comments: i % 17, blobs: map.empty()}
  return {stats, deep}
}

proc main() [fs] {
  var i = 0
  var total = 0

  while i < 20000 {
    let scan = make_scan(i)
    let report = {stats: scan.stats, name: "file.xsh"}
    total += scan.deep.blanks + scan.deep.code + scan.deep.comments
    total += report.stats.blanks + report.stats.code + report.stats.comments
    i += 1
  }

  print $total % 256
}

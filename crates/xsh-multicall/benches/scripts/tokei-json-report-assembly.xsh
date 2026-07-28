type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}
type FileReport = {stats: Stats, name: Str}

pure zero_stats() -> Stats {
  {blanks: 0, code: 0, comments: 0, blobs: map.empty()}
}

pure child_reports(reports: List[FileReport]) -> Map[Any] {
  var children: Map[List[FileReport]] = {}

  for report in reports {
    for language in report.stats.blobs.keys() {
      let stats = report.stats.blobs.get(language, zero_stats()).require(Stats) ?? zero_stats()
      children = children.push(language, {stats, name: report.name})
    }
  }

  var output: Map[Any] = {}
  for language in children.keys() |> sort {
    output[language] = children.get(language, [])
  }
  output
}

var reports: List[FileReport] = []
var index = 0
while index < 4000 {
  var blobs: Map[Any] = {}
  if index % 9 == 0 {
    blobs["Markdown"] = {blanks: 2, code: 4, comments: 1, blobs: map.empty()}
  }
  if index % 37 == 0 {
    blobs["TOML"] = {blanks: 1, code: 3, comments: 2, blobs: map.empty()}
  }
  reports = reports.push({
    stats: {blanks: index % 5, code: 10, comments: index % 3, blobs},
    name: f"file-${index}",
  })
  index += 1
}

print (json.encode(child_reports(reports))?)

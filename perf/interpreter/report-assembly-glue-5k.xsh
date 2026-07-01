type Stats = {blanks: Int, code: Int, comments: Int}

type FileReport = {stats: Stats, name: Str}

type ScannedFile = {language: Str, report: FileReport}

proc main() [fs, error] {
  var scanned: List[ScannedFile] = []
  var i = 0

  while i < 5000 {
    let language = if i % 5 == 0 {
      "Python"
    } else if i % 5 == 1 {
      "TSX"
    } else if i % 5 == 2 {
      "JSON"
    } else if i % 5 == 3 {
      "Markdown"
    } else {
      "HTML"
    }

    scanned = scanned.push(
      {language, report: {stats: {blanks: i % 7, code: i % 17, comments: i % 3}, name: f"/tmp/src/file-${i}"}},
    )

    i += 1
  }

  let languages = ["HTML", "JSON", "Markdown", "Python", "TSX"]
  var html_reports: List[FileReport] = []
  var json_reports: List[FileReport] = []
  var markdown_reports: List[FileReport] = []
  var python_reports: List[FileReport] = []
  var tsx_reports: List[FileReport] = []
  var total = 0

  for row in scanned {
    match row.language {
      "HTML" => html_reports = html_reports.push(row.report)
      "JSON" => json_reports = json_reports.push(row.report)
      "Markdown" => markdown_reports = markdown_reports.push(row.report)
      "Python" => python_reports = python_reports.push(row.report)
      "TSX" => tsx_reports = tsx_reports.push(row.report)
      _ => {}
    }
  }

  for language in languages {
    var reports: List[FileReport] = []

    match language {
      "HTML" => reports = html_reports
      "JSON" => reports = json_reports
      "Markdown" => reports = markdown_reports
      "Python" => reports = python_reports
      "TSX" => reports = tsx_reports
      _ => {}
    }

    total += reports.len()

    if reports.len() > 0 {
      total += reports[0].stats.code
    }
  }

  print $total % 256
}

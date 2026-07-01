type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}

type Candidate = {rel: Str, language: Str, text: Str}

type FileReport = {stats: Stats, name: Str}

pure fake_scan(text: Str) -> Stats {
  var blanks = 0
  var code = 0
  var comments = 0

  for line in text.lines() {
    let trimmed = line.trim()

    if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("#") or trimmed.starts_with("//") {
      comments += 1
    } else {
      code += 1
    }
  }

  var blobs: Map[Any] = {}

  if text.contains("embedded") {
    blobs["JavaScript"] = {blanks: 0, code: 1, comments: 0, blobs: map.empty()}
  }

  return {blanks, code, comments, blobs}
}

pure zero_stats() -> Stats {
  return {blanks: 0, code: 0, comments: 0, blobs: map.empty()}
}

pure hidden_relative(rel: Str) -> Bool {
  return rel.starts_with(".") or rel.contains("/.")
}

pure ignored_by_patterns(rel: Str, patterns: List[Str]) -> Bool {
  for raw in patterns {
    let pattern = raw.trim()

    if pattern != "" and ! pattern.starts_with("#") {
      if rel == pattern or rel.starts_with(f"${pattern}/") {
        return true
      }
    }
  }

  return false
}

proc main() {
  var candidates: List[Candidate] = []
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

    let text = if i % 23 == 0 {
      """<script>
embedded
</script>
"""
    } else if i % 7 == 0 {
      """# note
let x = 1

let y = 2
"""
    } else {
      """let x = 1
// note
let y = 2
"""
    }

    candidates = candidates.push({rel: f"src/file-${i}.xsh", language, text})
    i += 1
  }

  let ignore_patterns: List[Str] = []
  var html_reports: List[FileReport] = []
  var html_code = 0
  var html_has_blobs = false
  var json_reports: List[FileReport] = []
  var json_code = 0
  var markdown_reports: List[FileReport] = []
  var markdown_code = 0
  var markdown_has_blobs = false
  var python_reports: List[FileReport] = []
  var python_code = 0
  var tsx_reports: List[FileReport] = []
  var tsx_code = 0
  var total = zero_stats()

  for entry in candidates {
    continue when entry.language == "" or hidden_relative(entry.rel) or ignored_by_patterns(entry.rel, ignore_patterns)
    let stats = fake_scan(entry.text)
    let report = {stats, name: entry.rel}

    total = {
      blanks: total.blanks + stats.blanks,
      code: total.code + stats.code,
      comments: total.comments + stats.comments,
      blobs: map.empty(),
    }

    match entry.language {
      "HTML" => {
        html_code += stats.code

        if stats.blobs.keys().len() > 0 {
          html_has_blobs = true
        }

        html_reports = html_reports.push(report)
      }
      "JSON" => {
        json_code += stats.code
        json_reports = json_reports.push(report)
      }
      "Markdown" => {
        markdown_code += stats.code

        if stats.blobs.keys().len() > 0 {
          markdown_has_blobs = true
        }

        markdown_reports = markdown_reports.push(report)
      }
      "Python" => {
        python_code += stats.code
        python_reports = python_reports.push(report)
      }
      "TSX" => {
        tsx_code += stats.code
        tsx_reports = tsx_reports.push(report)
      }
      _ => {}
    }
  }

  var checksum = total.code + total.comments + total.blanks

  for language in ["HTML", "JSON", "Markdown", "Python", "TSX"] {
    var reports: List[FileReport] = []
    var code = 0
    var has_blobs = false

    match language {
      "HTML" => {
        reports = html_reports
        code = html_code
        has_blobs = html_has_blobs
      }
      "JSON" => {
        reports = json_reports
        code = json_code
      }
      "Markdown" => {
        reports = markdown_reports
        code = markdown_code
        has_blobs = markdown_has_blobs
      }
      "Python" => {
        reports = python_reports
        code = python_code
      }
      "TSX" => {
        reports = tsx_reports
        code = tsx_code
      }
      _ => {}
    }

    checksum += reports.len()
    checksum += code

    if has_blobs {
      checksum += 1
    }
  }

  print $checksum % 256
}

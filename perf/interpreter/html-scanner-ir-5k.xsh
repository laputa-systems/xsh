type Stats = {blanks: Int, code: Int, comments: Int}

pure count_code_text(text: Str) -> Stats {
  var blanks = 0
  var code = 0

  for line in text.lines() {
    if line.trim() == "" {
      blanks += 1
    } else {
      code += 1
    }
  }

  return {blanks, code, comments: 0}
}

pure count_slash_language(text: Str) -> Stats {
  var blanks = 0
  var code = 0
  var comments = 0

  for line in text.lines() {
    let trimmed = line.trim()

    if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("//") {
      comments += 1
    } else {
      code += 1
    }
  }

  return {blanks, code, comments}
}

pure count_html(text: Str) -> Stats {
  if ! text.contains("<!--") {
    let lower_text = text.lower()

    if ! lower_text.contains("<script") {
      return count_code_text(text)
    }
  }

  var blanks = 0
  var code = 0
  var comments = 0
  var in_comment = false
  var in_script = false
  var script_lines: List[Str] = []
  var javascript = {blanks: 0, code: 0, comments: 0}

  for line in text.lines() {
    let trimmed = line.trim()

    if in_script {
      let lower = trimmed.lower()

      if lower.starts_with("</script") {
        let scan = count_slash_language(script_lines.join("\n"))
        javascript.blanks += scan.blanks
        javascript.code += scan.code
        javascript.comments += scan.comments
        code += 1
        script_lines = []
        in_script = false
      } else {
        script_lines = script_lines.push(line)
      }
    } else if in_comment {
      comments += 1

      if trimmed.contains("-->") {
        in_comment = false
      }
    } else if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("<!--") {
      comments += 1

      if ! trimmed.contains("-->") {
        in_comment = true
      }
    } else {
      code += 1
      let lower = trimmed.lower()

      if lower.starts_with("<script") and ! lower.contains("</script") {
        in_script = true
        script_lines = []
      }
    }
  }

  if in_script {
    let scan = count_slash_language(script_lines.join("\n"))
    javascript.blanks += scan.blanks
    javascript.code += scan.code
    javascript.comments += scan.comments
  }

  var stats = {blanks, code, comments}
  stats.blanks += javascript.blanks
  stats.code += javascript.code
  stats.comments += javascript.comments
  return stats
}

let plain = """
<section>
  <h1>Title</h1>
  <p>alpha beta gamma</p>
  <p>delta epsilon zeta</p>
</section>

<section>
  <h2>Next</h2>
  <p>eta theta iota</p>
  <p>kappa lambda mu</p>
</section>
"""

let scripted = """
<html>
<body>
<script>
const value = 1
// note
const other = 2
</script>
<!-- trailing note -->
</body>
</html>
"""

var i = 0
var total = 0

while i < 5000 {
  let plain_stats = count_html(plain)
  let scripted_stats = count_html(scripted)
  total += plain_stats.code + plain_stats.blanks + scripted_stats.code + scripted_stats.comments
  i += 1
}

print $total % 256

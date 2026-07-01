type Stats = {blanks: Int, code: Int, comments: Int, blobs: Map[Any]}

type Scan = {stats: Stats, deep: Stats}

pure scan_hash(text: Str) -> Scan {
  var blanks = 0
  var code = 0
  var comments = 0

  for line in text.lines() {
    let trimmed = line.trim()

    if trimmed == "" {
      blanks += 1
    } else if trimmed.starts_with("#") {
      comments += 1
    } else {
      code += 1
    }
  }

  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

pure scan_slash(text: Str) -> Scan {
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

  let stats = {blanks, code, comments, blobs: map.empty()}
  return {stats, deep: stats}
}

pure scan_json(text: Str) -> Scan {
  let stats = {blanks: 0, code: text.count_lines(), comments: 0, blobs: map.empty()}
  return {stats, deep: stats}
}

pure scan_markup(text: Str) -> Scan {
  var blanks = 0
  var code = 0

  for line in text.lines() {
    if line.trim() == "" {
      blanks += 1
    } else {
      code += 1
    }
  }

  let stats = {blanks, code, comments: 0, blobs: map.empty()}
  return {stats, deep: stats}
}

pure count_language(language: Str, text: Str) -> Scan {
  if language == "TSX" {
    return scan_slash(text)
  }

  if language == "Python" {
    return scan_hash(text)
  }

  if language == "JSON" {
    return scan_json(text)
  }

  if language == "TypeScript" {
    return scan_slash(text)
  }

  if language == "PO File" {
    return scan_hash(text)
  }

  if language == "HTML" {
    return scan_markup(text)
  }

  if language == "SVG" {
    return scan_markup(text)
  }

  if language == "MDX" {
    return scan_markup(text)
  }

  if language == "Markdown" {
    return scan_markup(text)
  }

  if language == "LESS" {
    return scan_slash(text)
  }

  if language == "JavaScript" {
    return scan_slash(text)
  }

  if language == "YAML" {
    return scan_hash(text)
  }

  if language == "BASH" {
    return scan_hash(text)
  }

  if language == "CSS" {
    return scan_slash(text)
  }

  if language == "Dockerfile" {
    return scan_hash(text)
  }

  if language == "Forge Config" {
    return scan_hash(text)
  }

  if language == "INI" {
    return scan_hash(text)
  }

  if language == "Lua" {
    return scan_hash(text)
  }

  if language == "Makefile" {
    return scan_hash(text)
  }

  if language == "Modelica" {
    return scan_slash(text)
  }

  if language == "Plain Text" {
    return scan_markup(text)
  }

  if language == "ReStructuredText" {
    return scan_markup(text)
  }

  if language == "Rust" {
    return scan_slash(text)
  }

  if language == "Shell" {
    return scan_hash(text)
  }

  if language == "Templ" {
    return scan_slash(text)
  }

  if language == "TOML" {
    return scan_hash(text)
  }

  if language == "XML" {
    return scan_markup(text)
  }

  return scan_hash(text)
}

proc main() [fs] {
  let text = """let a = 1
// note
let b = 2
let c = 3

let d = 4
// note
let e = 5
let f = 6
let g = 7
// note
let h = 8
"""

  var total = 0
  var i = 0

  while i < 5000 {
    let language = if i % 27 == 0 {
      "Python"
    } else if i % 27 == 1 {
      "TSX"
    } else if i % 27 == 2 {
      "JSON"
    } else if i % 27 == 3 {
      "TypeScript"
    } else if i % 27 == 4 {
      "PO File"
    } else if i % 27 == 5 {
      "HTML"
    } else if i % 27 == 6 {
      "SVG"
    } else if i % 27 == 7 {
      "MDX"
    } else if i % 27 == 8 {
      "Markdown"
    } else if i % 27 == 9 {
      "LESS"
    } else if i % 27 == 10 {
      "JavaScript"
    } else if i % 27 == 11 {
      "YAML"
    } else if i % 27 == 12 {
      "BASH"
    } else if i % 27 == 13 {
      "CSS"
    } else if i % 27 == 14 {
      "Dockerfile"
    } else if i % 27 == 15 {
      "Forge Config"
    } else if i % 27 == 16 {
      "INI"
    } else if i % 27 == 17 {
      "Lua"
    } else if i % 27 == 18 {
      "Makefile"
    } else if i % 27 == 19 {
      "Modelica"
    } else if i % 27 == 20 {
      "Plain Text"
    } else if i % 27 == 21 {
      "ReStructuredText"
    } else if i % 27 == 22 {
      "Rust"
    } else if i % 27 == 23 {
      "Shell"
    } else if i % 27 == 24 {
      "Templ"
    } else if i % 27 == 25 {
      "TOML"
    } else {
      "XML"
    }

    let scan = if language == "TSX" {
      scan_slash(text)
    } else if language == "Python" {
      scan_hash(text)
    } else if language == "JSON" {
      scan_json(text)
    } else if language == "TypeScript" {
      scan_slash(text)
    } else if language == "PO File" {
      scan_hash(text)
    } else if language == "HTML" {
      scan_markup(text)
    } else if language == "SVG" {
      scan_markup(text)
    } else if language == "MDX" {
      scan_markup(text)
    } else if language == "Markdown" {
      scan_markup(text)
    } else if language == "LESS" {
      scan_slash(text)
    } else if language == "JavaScript" {
      scan_slash(text)
    } else if language == "YAML" {
      scan_hash(text)
    } else if language == "BASH" {
      scan_hash(text)
    } else if language == "CSS" {
      scan_slash(text)
    } else if language == "Dockerfile" {
      scan_hash(text)
    } else if language == "Forge Config" {
      scan_hash(text)
    } else if language == "INI" {
      scan_hash(text)
    } else if language == "Lua" {
      scan_hash(text)
    } else if language == "Makefile" {
      scan_hash(text)
    } else if language == "Modelica" {
      scan_slash(text)
    } else if language == "Plain Text" {
      scan_markup(text)
    } else if language == "ReStructuredText" {
      scan_markup(text)
    } else if language == "Rust" {
      scan_slash(text)
    } else if language == "Shell" {
      scan_hash(text)
    } else if language == "Templ" {
      scan_slash(text)
    } else if language == "TOML" {
      scan_hash(text)
    } else if language == "XML" {
      scan_markup(text)
    } else {
      count_language(language, text)
    }

    total += scan.deep.code + scan.deep.comments + scan.deep.blanks
    i += 1
  }

  print $total % 256
}

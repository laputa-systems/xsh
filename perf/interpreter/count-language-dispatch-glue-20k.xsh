pure scan_kind(language: Str) -> Int {
  if language == "TSX" {
    return 1
  }

  if language == "Python" {
    return 2
  }

  if language == "JSON" {
    return 3
  }

  if language == "TypeScript" {
    return 4
  }

  if language == "PO File" {
    return 5
  }

  if language == "HTML" {
    return 6
  }

  if language == "SVG" {
    return 7
  }

  if language == "MDX" {
    return 8
  }

  if language == "Markdown" {
    return 9
  }

  if language == "LESS" {
    return 10
  }

  if language == "JavaScript" {
    return 11
  }

  if language == "YAML" {
    return 12
  }

  if language == "BASH" {
    return 13
  }

  if language == "CSS" {
    return 14
  }

  if language == "Dockerfile" {
    return 15
  }

  if language == "Forge Config" {
    return 16
  }

  if language == "INI" {
    return 17
  }

  if language == "Lua" {
    return 18
  }

  if language == "Makefile" {
    return 19
  }

  if language == "Modelica" {
    return 20
  }

  if language == "Plain Text" {
    return 21
  }

  if language == "ReStructuredText" {
    return 22
  }

  if language == "Rust" {
    return 23
  }

  if language == "Shell" {
    return 24
  }

  if language == "Templ" {
    return 25
  }

  if language == "TOML" {
    return 26
  }

  if language == "XML" {
    return 27
  }

  return 0
}

proc main() {
  var total = 0
  var i = 0

  while i < 20000 {
    let language = if i % 10 == 0 {
      "Python"
    } else if i % 10 == 1 {
      "TSX"
    } else if i % 10 == 2 {
      "JSON"
    } else if i % 10 == 3 {
      "TypeScript"
    } else if i % 10 == 4 {
      "PO File"
    } else if i % 10 == 5 {
      "HTML"
    } else if i % 10 == 6 {
      "SVG"
    } else if i % 10 == 7 {
      "MDX"
    } else if i % 10 == 8 {
      "LESS"
    } else {
      "JavaScript"
    }

    total += scan_kind(language)
    i += 1
  }

  print $total % 256
}

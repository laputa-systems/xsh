pure lang_for_name(ext: Str, name: Str) -> Str {
  if name == "dockerfile" {
    return "Dockerfile"
  }

  if name == "makefile" {
    return "Makefile"
  }

  if name == "post-checkout" or name == "post-merge" or name == "upload_snapshots" {
    return "BASH"
  }

  if ext == "bash" {
    return "BASH"
  }

  if ext == "tsx" {
    return "TSX"
  }

  if ext == "py" {
    return "Python"
  }

  if ext == "json" {
    return "JSON"
  }

  if ext == "ts" or ext == "cts" or ext == "mts" {
    return "TypeScript"
  }

  if ext == "po" {
    return "PO File"
  }

  if ext == "html" or ext == "htm" {
    return "HTML"
  }

  if ext == "svg" {
    return "SVG"
  }

  if ext == "mdx" {
    return "MDX"
  }

  if ext == "md" or ext == "markdown" {
    return "Markdown"
  }

  if ext == "less" {
    return "LESS"
  }

  if ext == "js" or ext == "cjs" or ext == "mjs" {
    return "JavaScript"
  }

  if ext == "yaml" or ext == "yml" {
    return "YAML"
  }

  if ext == "css" {
    return "CSS"
  }

  if ext == "cfg" {
    return "Forge Config"
  }

  if ext == "ini" {
    return "INI"
  }

  if ext == "lua" {
    return "Lua"
  }

  if ext == "mo" {
    return "Modelica"
  }

  if ext == "rs" {
    return "Rust"
  }

  if ext == "rst" {
    return "ReStructuredText"
  }

  if ext == "sh" {
    return "Shell"
  }

  if ext == "templ" or ext == "tmpl" {
    return "Templ"
  }

  if ext == "toml" {
    return "TOML"
  }

  if ext == "xml" {
    return "XML"
  }

  if ext == "txt" {
    return "Plain Text"
  }

  return ""
}

proc main() {
  var total = 0
  var i = 0

  while i < 20000 {
    let ext = if i % 10 == 0 {
      "py"
    } else if i % 10 == 1 {
      "tsx"
    } else if i % 10 == 2 {
      "json"
    } else if i % 10 == 3 {
      "ts"
    } else if i % 10 == 4 {
      "po"
    } else if i % 10 == 5 {
      "html"
    } else if i % 10 == 6 {
      "svg"
    } else if i % 10 == 7 {
      "mdx"
    } else if i % 10 == 8 {
      "less"
    } else {
      "js"
    }

    total += lang_for_name(ext, f"file-${i}.${ext}").byte_len()
    i += 1
  }

  print $total % 256
}

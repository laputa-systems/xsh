pure source_exts() -> List[Str] {
  return [
    "",
    "json",
    "py",
    "tsx",
    "ts",
    "cts",
    "mts",
    "po",
    "html",
    "htm",
    "svg",
    "mdx",
    "md",
    "markdown",
    "less",
    "js",
    "cjs",
    "mjs",
    "yaml",
    "yml",
    "css",
    "cfg",
    "ini",
    "lua",
    "mo",
    "rs",
    "rst",
    "sh",
    "templ",
    "tmpl",
    "toml",
    "xml",
    "bash",
    "txt",
  ]
}

pure ext_allowed(ext: Str, exts: List[Str]) -> Bool {
  for candidate in exts {
    if candidate == ext {
      return true
    }
  }

  return false
}

proc main() {
  let exts = source_exts()
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

    if ext_allowed(ext, exts) {
      total += ext.byte_len()
    }

    i += 1
  }

  print $total % 256
}

#!/usr/bin/env -S xsh --
# Secret Scan
# Scan source-like files for common secret patterns.
# Usage: xsh showcase/secret-scan.xsh -- [--root DIR] [--ext EXT]
# Example: xsh showcase/secret-scan.xsh -- --root .
type Finding = {file: Str, line: Int, kind: Str, text: Str}

type Opts = {root: Path, ext: List[Str], verbose: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {
        form: "--root DIR",
        default: p".",
      },
      ext: {
        form: "--ext EXT",
        repeated: true,
      },
      verbose: {
        form: "--verbose",
        default: false,
      },
    },
  )?

  let root = if opts.root == "." {
    if let Ok(r) = fs.gitroot() {
      r
    } else {
      fs.cwd()?
    }
  } else {
    opts.root.resolve()?
  }

  let scan_exts = if ! opts.ext.is_empty() {
    opts.ext
  } else {
    [
      "rs",
      "go",
      "py",
      "js",
      "ts",
      "sh",
      "env",
      "toml",
      "yaml",
      "yml",
      "json",
      "txt",
      "xsh",
      "cfg",
      "ini",
    ]
  }

  let scan_ext_set: Set[Str] = set.from(scan_exts)

  # Static patterns are prepared before scanning.
  let patterns = [
    {
      kind: "aws-key",
      re: rx"AKIA[0-9A-Z]{16}",
    },
    {
      kind: "private-key",
      re: rx"-----BEGIN .* PRIVATE KEY-----",
    },
    {
      kind: "api-key",
      re: rx"""(?i)(api[_-]?key|secret[_-]?key)\s*[:=]\s*['"][A-Za-z0-9\-_]{16,}['"]""",
    },
    {
      kind: "jwt",
      re: rx"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
    },
    {
      kind: "gh-token",
      re: rx"gh[pousr]_[A-Za-z0-9]{36}",
    },
  ]

  let files = fs.files(root)
    |> where .path.ext() in scan_ext_set
    |> sort-by .path

  if opts.verbose {
    print f"scanning {files.len()} files in {root}"
  }

  let findings: List[Finding] = files
    |> par-map { |entry|
      let hits: List[Finding] = collect {
        if let Ok(src) = entry.path.read_text() {
          let rel = entry.path.relative_to(root).display()

          for item in src.lines() |> enumerate() {
            let line_num = item.index + 1
            let line = item.value

            for pattern in patterns {
              if pattern.re.matches(line) {
                yield {file: rel, line: line_num, kind: pattern.kind, text: line.trim()}
              }
            }
          }
        }
      }

      hits
    }
    |> flat-map { |hits|
      hits
    }

  for finding in findings {
    print f"{finding.file}:{finding.line}: [{finding.kind}] {finding.text}"
  }

  let files_hit = findings
    |> group-by .file
    |> count()

  print f"{findings.len()} findings in {files_hit} files ({files.len()} scanned)"
}

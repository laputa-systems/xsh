#!/usr/bin/env -S xsh --
type Opts = {root: Path, fail: Bool, show_ok: Bool}

type Finding = {severity: Int, kind: Str, path: Str, detail: Str}

pure inside_root(path_text: Str, root_text: Str) -> Bool {
  return path_text == root_text or path_text.starts_with(f"${root_text}/")
}

pure severity_label(severity: Int) -> Str {
  if severity == 1 {
    return "error"
  }

  if severity == 2 {
    return "warn"
  }

  return "info"
}

pure add_finding(findings: List[Finding], severity: Int, kind: Str, item_path: Str, detail: Str) -> List[Finding] {
  return findings.push({severity, kind, path: item_path, detail})
}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      root: {
        form: "--root DIR",
        default: p".",
      },
      fail: {
        form: "--fail",
        default: false,
      },
      show_ok: {
        form: "--show-ok",
        default: false,
      },
    },
  )?

  let root = opts.root.resolve()?
  let root_text = root.display()
  let current = user.current()?
  var findings: List[Finding] = []

  for entry in fs.walk(root, stat: true)? {
    let rel = entry.path.relative_to(root).display()
    let shown = if rel == "" { "." } else { rel }

    if entry.kind == "symlink" {
      let target = entry.path.readlink()?

      if target.display().starts_with("/") {
        findings = add_finding(findings, 2, "absolute-symlink", shown, target.display())
      }

      match entry.path.resolve() {
        Ok(resolved) => {
          if ! inside_root(resolved.display(), root_text) {
            findings = add_finding(findings, 1, "escaping-symlink", shown, resolved.display())
          }
        }
        Err(_) => findings = add_finding(findings, 1, "broken-symlink", shown, target.display())
      }
    }

    if entry.kind == "file" and entry.world_writable {
      findings = add_finding(findings, 2, "world-writable-file", shown, f"mode ${entry.mode}")
    }

    if entry.kind == "dir" and entry.world_writable and ! entry.sticky {
      findings = add_finding(findings, 1, "world-writable-dir", shown, f"mode ${entry.mode}")
    }

    if entry.kind == "file" and (entry.setuid or entry.setgid) {
      findings = add_finding(findings, 1, "setuid-setgid-file", shown, f"mode ${entry.mode}")
    }

    if entry.kind == "file" and entry.executable and entry.uid != current.uid {
      findings = add_finding(findings, 2, "foreign-executable", shown, f"uid ${entry.uid}")
    }
  }

  let rows = findings |> sort-by f"${.severity}:${.path}:${.kind}"

  for row in rows {
    print f"${severity_label(row.severity)} ${row.kind} ${row.path} ${row.detail}"
  }

  if rows.len() == 0 and opts.show_ok {
    print f"ok scanned ${root}"
  }

  if rows.len() > 0 and opts.fail {
    abort(1)
  }
}

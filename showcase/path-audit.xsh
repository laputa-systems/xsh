#!/usr/bin/env -S xsh --
type Opts = {env_var: Str, fail: Bool, show_ok: Bool, duplicates_only: Bool}

type DirFinding = {severity: Int, kind: Str, path: Str, detail: Str}

type ShadowFinding = {name: Str, path: Str, detail: Str}

type SeenCommand = {name: Path, path: Str}

pure severity_label(severity: Int) -> Str {
  if severity == 1 {
    return "error"
  }

  if severity == 2 {
    return "warn"
  }

  return "info"
}

pure add_dir_finding(
  findings: List[DirFinding],
  severity: Int,
  kind: Str,
  item_path: Str,
  detail: Str,
) -> List[DirFinding] {
  return findings.push({severity, kind, path: item_path, detail})
}

proc main(...argv: List[Str]) [fs, env, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      env_var: {
        form: "--var NAME",
        default: "PATH",
      },
      fail: {
        form: "--fail",
        default: false,
      },
      show_ok: {
        form: "--show-ok",
        default: false,
      },
      duplicates_only: {
        form: "--duplicates-only",
        default: false,
      },
    },
  )?

  let parts = env.path_entries(opts.env_var)?
  var dir_findings: List[DirFinding] = []
  var valid_dirs: List[Path] = []
  var seen_dirs: Map[List[Path]] = {}

  for part in parts {
    let label = if part.empty { f"${opts.env_var}[${part.index}]" } else { part.raw }

    if part.empty {
      if ! opts.duplicates_only {
        dir_findings = add_dir_finding(dir_findings, 1, "empty-entry", label, "current directory")
      }

      continue
    }

    let path_value = part.path

    if ! path_value.exists()? {
      if ! opts.duplicates_only {
        dir_findings = add_dir_finding(dir_findings, 1, "missing-directory", label, "missing")
      }

      continue
    }

    let resolved = path_value.resolve()?
    let meta = resolved.metadata()?

    if meta.kind != "dir" {
      if ! opts.duplicates_only {
        dir_findings = add_dir_finding(dir_findings, 1, "not-directory", label, meta.kind)
      }

      continue
    }

    let resolved_text = resolved.display()

    let same_display = seen_dirs.get(resolved_text, [])

    if resolved in same_display {
      dir_findings = add_dir_finding(dir_findings, 2, "duplicate-directory", label, resolved_text)
    } else {
      seen_dirs[resolved_text] = same_display.push(resolved)

      if ! opts.duplicates_only and meta.executable {
        valid_dirs = valid_dirs.push(resolved)
      }
    }

    if ! opts.duplicates_only {
      if meta.world_writable {
        dir_findings = add_dir_finding(dir_findings, 1, "world-writable-directory", label, f"mode ${meta.mode}")
      }

      if ! meta.executable {
        dir_findings = add_dir_finding(dir_findings, 1, "non-executable-directory", label, f"mode ${meta.mode}")
      }
    }
  }

  var shadows: List[ShadowFinding] = []

  if ! opts.duplicates_only {
    var first_path: Map[List[SeenCommand]] = {}

    for dir in valid_dirs {
      for child in fs.children(dir, ordered: false)? {
        continue when child.kind != "file" or ! child.executable
        let child_path = child.path.display()
        let exact_name = child.path.strip_prefix(dir)?
        let same_display = first_path.get(child.name, [])
        let earlier = same_display |> where .name == exact_name

        if earlier.len() > 0 {
          shadows = shadows.push(
            {name: child.name, path: child_path, detail: f"shadows ${earlier[0].path}"},
          )
        } else {
          first_path[child.name] = same_display.push({name: exact_name, path: child_path})
        }
      }
    }
  }

  let dir_rows = dir_findings |> sort-by f"${.severity}:${.path}:${.kind}"
  let shadow_rows = shadows |> sort-by f"${.name}:${.path}"

  if dir_rows.len() > 0 {
    print "Directory problems"

    for row in dir_rows {
      print f"${severity_label(row.severity)} ${row.kind} ${row.path} ${row.detail}"
    }
  }

  if shadow_rows.len() > 0 {
    print "Command shadowing"

    for row in shadow_rows {
      print f"warn shadowed-command ${row.name} ${row.path} ${row.detail}"
    }
  }

  if dir_rows.len() == 0 and shadow_rows.len() == 0 and opts.show_ok {
    print f"ok ${opts.env_var} entries=${parts.len()}"
  }

  if (dir_rows.len() > 0 or shadow_rows.len() > 0) and opts.fail {
    abort(1)
  }
}

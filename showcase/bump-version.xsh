#!/usr/bin/env -S xsh --
# Bump Version
# Bump a Cargo package version by major, minor, or patch component.
# Edit only the version field in [package] and publish the file atomically.
# Usage: xsh showcase/bump-version.xsh -- COMPONENT [--manifest PATH] [--dry-run=false]
# Example: xsh showcase/bump-version.xsh -- patch --manifest Cargo.toml
type Opts = {component: Str, manifest: Path, dry_run: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      component: {
        form: "COMPONENT",
        choices: [
          "major",
          "minor",
          "patch",
        ],
        help: "major | minor | patch",
      },
      manifest: {
        form: "--manifest PATH",
        default: p"Cargo.toml",
      },
      dry_run: {
        form: "--dry-run",
        default: true,
      },
    },
  )?

  if ! opts.manifest.exists()? {
    print f"error: ${opts.manifest.display()} not found"
    abort(1)
  }

  let manifest = opts.manifest.resolve()?
  let content = manifest.read_text()?

  let version_re = regex.compile("^[ \\t]*version[ \\t]*=[ \\t]*\"(\\d+)\\.(\\d+)\\.(\\d+)\"")?
  let section_re = regex.compile("^[ \\t]*\\[([^\\]]+)\\]")?
  var old_line = ""
  var version_line_index = -1
  var major = 0
  var minor = 0
  var patch_component = 0
  var in_package = false

  for item in content.split("\n") |> enumerate() {
    let line = item.value
    let section = section_re.captures(line)
    if section.len() >= 2 {
      in_package = section[1] == "package"
    }

    if in_package {
      let caps = version_re.captures(line)

      if caps.len() >= 4 {
        old_line = line
        version_line_index = item.index
        major = json.decode(caps[1])?
        minor = json.decode(caps[2])?
        patch_component = json.decode(caps[3])?
        break
      }
    }
  }

  if version_line_index < 0 {
    print "error: no version field found in [package] section"
    abort(1)
  }

  let old_version = f"${major}.${minor}.${patch_component}"
  let new_major = if opts.component == "major" { major + 1 } else { major }
  let new_minor = if opts.component == "major" { 0 } else { if opts.component == "minor" { minor + 1 } else { minor } }
  let new_patch = if opts.component == "patch" { patch_component + 1 } else { 0 }
  let new_version = f"${new_major}.${new_minor}.${new_patch}"
  let old_version_field = f"\"${old_version}\""
  let parts = old_line.split(old_version_field, maxsplit: 1)
  let new_line = f"${parts[0]}\"${new_version}\"${parts[1]}"
  print $manifest
  print f"  ${old_version} → ${new_version}  (${opts.component} bump)"

  if opts.dry_run {
    print "dry run \u{2014} not writing"
    return
  }

  let updated = [
    if item.index == version_line_index { new_line } else { item.value }
    for item in content.split("\n") |> enumerate()
  ]

  let new_content = updated.join("\n")
  manifest.write_atomic(new_content)?
  print "updated"
}

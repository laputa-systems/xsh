#!/usr/bin/env -S xsh --
# Bump Version
# Bump a Cargo package version by major, minor, or patch component.
# Usage: xsh showcase/bump-version.xsh -- COMPONENT [--manifest PATH] [--dry-run=false]
# Example: xsh showcase/bump-version.xsh -- patch --manifest Cargo.toml
type Opts = {component: Str, manifest: Path, dry_run: Bool}

proc main(...argv: List[Str]) [fs, error] {
  let opts: Opts = cli.parse(
    argv,
    {
      component: {form: "COMPONENT", choices: ["major", "minor", "patch"], help: "major | minor | patch"},
      manifest: {form: "--manifest PATH", default: p"Cargo.toml"},
      dry_run: {form: "--dry-run", default: true},
    },
  )?

  if opts.component != "major" and opts.component != "minor" and opts.component != "patch" {
    print f"error: component must be major, minor, or patch (got: ${opts.component})"
    return
  }

  let manifest = opts.manifest.resolve()?

  if ! manifest.exists()? {
    print f"error: ${manifest.display()} not found"
    return
  }

  let content = manifest.read_text()?

  # Match `version = "X.Y.Z"` only in the [package] section (before [dependencies])
  let version_re = regex.compile("^version = \"(\\d+)\\.(\\d+)\\.(\\d+)\"")?
  let dep_section_re = regex.compile("^\\[dependencies")?
  var old_line = ""
  var major = 0
  var minor = 0
  var patch_component = 0
  var found = false
  var in_package = true

  for line in content.lines() {
    if dep_section_re.matches(line) {
      in_package = false
    }

    if in_package {
      let caps = version_re.captures(line)

      if caps.len() >= 4 {
        old_line = line
        major = json.decode(caps[1])?
        minor = json.decode(caps[2])?
        patch_component = json.decode(caps[3])?
        found = true
        break
      }
    }
  }

  if ! found {
    print "error: no version field found in [package] section"
    return
  }

  let old_version = f"${major}.${minor}.${patch_component}"
  let new_major = if opts.component == "major" { major + 1 } else { major }
  let new_minor = if opts.component == "major" { 0 } else { if opts.component == "minor" { minor + 1 } else { minor } }
  let new_patch = if opts.component == "patch" { patch_component + 1 } else { 0 }
  let new_version = f"${new_major}.${new_minor}.${new_patch}"
  let new_line = f"version = \"${new_version}\""
  print $manifest
  print f"  ${old_version} → ${new_version}  (${opts.component} bump)"

  if opts.dry_run {
    print "dry run \u{2014} not writing"
    return
  }

  let new_content = content.replace(old_line, new_line)
  manifest.write_atomic(new_content)?
  print "updated"
}

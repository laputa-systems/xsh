proc test_bump_version_usage() [process, error] {
  let output = run.text "xsh" "showcase/bump-version.xsh" -- --help ?
  test.contains(output, "usage:")?
  test.contains(output, "major | minor | patch")?
}

proc test_bump_version_updates_only_the_package_field(ctx: TestContext) [fs, process, error] {
  let manifest = test.temp_file(
    ctx,
    name: "Cargo.toml",
    contents: b"[workspace.package]\nversion = \"1.2.3\"\n\n[package]\nname = \"demo\"\nversion = \"1.2.3\" # keep comment\n",
  )?
  let status = run.status "xsh" "showcase/bump-version.xsh" -- patch --manifest $manifest --dry-run=false
  test.ok(status.exited_with(0))?
  test.eq(
    manifest.read_text()?,
    """[workspace.package]
version = "1.2.3"

[package]
name = "demo"
version = "1.2.4" # keep comment
""",
  )?
}

proc test_bump_version_requires_a_package_version(ctx: TestContext) [fs, process, error] {
  let original = """[workspace.package]
version = "1.2.3"
"""
  let manifest = test.temp_file(
    ctx,
    name: "workspace-only.toml",
    contents: b"[workspace.package]\nversion = \"1.2.3\"\n",
  )?
  let status = run.status "xsh" "showcase/bump-version.xsh" -- patch --manifest $manifest --dry-run=false
  test.ok(! status.exited_with(0), "workspace version is not a package version")?
  test.eq(manifest.read_text()?, original)?
}

proc test_bump_version_rejects_missing_or_malformed_package_version(ctx: TestContext) [fs, process, error] {
  let missing = test.temp_path(ctx, name: "missing-Cargo.toml")
  let missing_status = run.status "xsh" "showcase/bump-version.xsh" -- patch --manifest $missing --dry-run=false
  test.ok(! missing_status.exited_with(0), "missing manifest must fail")?
  test.ok(! missing.exists()?)?

  let original = """[package]
version = "invalid"
"""
  let malformed = test.temp_file(ctx, name: "malformed-Cargo.toml", contents: b"[package]\nversion = \"invalid\"\n")?
  let malformed_status = run.status "xsh" "showcase/bump-version.xsh" -- patch --manifest $malformed --dry-run=false
  test.ok(! malformed_status.exited_with(0), "malformed package version must fail")?
  test.eq(malformed.read_text()?, original)?
}

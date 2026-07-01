pure xsh_bin() -> Path {
  return p"target/debug/xsh"
}

proc test_secret_scan(ctx: TestContext) [fs, process, error] {
  let root = test.temp_dir(ctx, name: "scan")?

  fp"${root}/creds.py".write("""AKIA1234567890ABCDEF
api_key = 'abcdefghijklmnop'
""")?

  let output = run.text xsh_bin() "showcase/secret-scan.xsh" -- --root $root ?
  test.contains(output, "[aws-key]")?
  test.contains(output, "[api-key]")?
  test.contains(output, "scanned")?
}

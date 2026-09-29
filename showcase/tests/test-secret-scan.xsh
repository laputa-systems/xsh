test test_secret_scan [fs, process, error] { |ctx|
  let root = test.temp_dir(ctx, name: "scan")?

  fp"${root}/creds.py".write("""AKIA1234567890ABCDEF
api_key = 'abcdefghijklmnop'
""")?

  let output = run.text "xsh" "showcase/secret-scan.xsh" -- --root $root ?
  test.contains(output, "[aws-key]")?
  test.contains(output, "[api-key]")?
  test.contains(output, "scanned")?
}

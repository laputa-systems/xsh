test test_secret_scan { |ctx|
  let root = test.temp_dir(ctx, name: "scan")?

  fp"${root}/creds.py".write("""AKIA1234567890ABCDEF
api_key = 'abcdefghijklmnop'
""")?

  let output = run.text "xsh" "showcase/secret-scan.xsh" -- --root $root ?
  "[aws-key]" in output
  "[api-key]" in output
  "scanned" in output
}

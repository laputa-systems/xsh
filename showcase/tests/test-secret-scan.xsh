test test_secret_scan { |ctx|
  let root = test.temp_dir(ctx, name: "scan")?

  fp"{root}/creds.py".write("""AKIA1234567890ABCDEF
api_key = 'abcdefghijklmnop'
""")?

  let output = run.text "xsh" "showcase/secret-scan.xsh" -- --root $root ?
  assert "[aws-key]" in output
  assert "[api-key]" in output
  assert "scanned" in output
}

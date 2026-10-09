test test_bytes_inspect { |ctx|
  let text_file = test.temp_file(ctx, name: "hello.txt", contents: b"hello world\n")?
  let output = run.text "xsh" "showcase/bytes-inspect.xsh" $text_file ?
  assert "sha256:" in output
  assert "base64:" in output
  assert "text:" in output
}

proc test_bytes_inspect(ctx: TestContext) [fs, process, error] {
  let text_file = test.temp_file(ctx, name: "hello.txt", contents: b"hello world\n")?
  let output = run.text "xsh" "showcase/bytes-inspect.xsh" -- $text_file ?
  test.contains(output, "sha256:")?
  test.contains(output, "base64:")?
  test.contains(output, "text:")?
}

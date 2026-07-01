proc test_elf_inspect(ctx: TestContext) [fs, error] {
  let plain = test.temp_file(ctx, name: "plain.txt", contents: b"plain")?
  test.eq(elf.inspect(plain)?.type, "not-elf")?
  let bad = test.temp_file(ctx, name: "bad-elf.bin", contents: b"\x7fELF\x02\x01")?
  test.error_kind(elf.inspect(bad), "elf-malformed")?
}

proc test_hash_digests_checksums_and_digest_methods(ctx: TestContext) [fs, error] {
  let data_path = test.temp_path(ctx, name: "hash-data.txt")
  fs.write(data_path, "abc")?
  let digest = hash.sha256(b"abc")
  test.eq(digest.base64(), "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=")?
  test.eq(hash.sha256(data_path)?.hex(), digest.hex())?
  test.eq(hash.md5(b"abc").hex(), "900150983cd24fb0d6963f7d28e17f72")?
  test.eq(hash.sha1(b"abc").hex(), "a9993e364706816aba3e25717850c26c9cd0d89d")?

  test.eq(
    hash.sha512(b"abc").hex(),
    "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
  )?

  test.eq(hash.crc32(b"123456789"), 3421780262)?
  test.eq(hash.crc32c(b"123456789"), 3808858755)?
  let check = hash.parse_check_line(f"${digest.hex()}  ${data_path.name()}")?
  test.eq(check.hex, digest.hex())?
  test.eq(check.path, data_path.name())?
  hash.verify_file(data_path, sha256: digest.hex())?
  test.error_kind(hash.verify_file(data_path, sha256: "00"), "checksum-format")?
  test.error_kind(
    hash.verify_file(data_path, sha256: "0000000000000000000000000000000000000000000000000000000000000000"),
    "checksum-mismatch",
  )?
}

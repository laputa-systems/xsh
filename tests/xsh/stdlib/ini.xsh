proc test_ini_decode_encode_and_files(ctx: TestContext) [fs, error] {
  let config = ini.decode("""global: root
[server]
Host = example.test
message = hello
  world
""")?

  test.eq(config.global, "root")?
  test.eq(config.server.host, "example.test")?

  test.eq(
    config.server.message,
    """hello
world""",
  )?

  let encoded = ini.encode({server: {message: config.server.message, host: config.server.host}, global: config.global})?
  test.contains(encoded, "global = root")?
  test.contains(encoded, "[server]")?
  test.contains(encoded, "host = example.test")?
  let config_path = test.temp_path(ctx, name: "app.ini")
  ini.write(config_path, {global: "root", server: {host: "example.test"}})?
  let read_back = ini.read(config_path)?
  test.eq(read_back.server.host, "example.test")?
  test.error_kind(ini.write(config_path, {global: "again"}, overwrite: false), "ini-write")?
}

use stdlib.source_locks as locks

test test_source_lock_verifies_size_and_sha256 { |ctx|
  let root = test.temp_dir(ctx, name: "source-lock")?
  let lock = fp"{root}/sources.lock.json"
  let input = fp"{root}/input"
  input.write("hello")
  lock.write("{\"schema_version\":1,\"inputs\":[{\"name\":\"sample\",\"version\":\"1\",\"url\":\"https://example.com/a\",\"sha256\":\"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\",\"size\":5,\"extra\":123}]}")
  let entries = locks.load_sources(lock)?
  assert entries[0].data.get("extra")? == 123
  assert locks.verify_file(input, entries[0]) is Ok(_)
  input.write("world")
  match locks.verify_file(input, entries[0]) {
    Ok(_) => fail "wrong hash was accepted"
    Err(failure) => assert failure.message.find("sha256 mismatch") != null
  }
  input.write("hello!")
  match locks.verify_file(input, entries[0]) {
    Ok(_) => fail "wrong size was accepted"
    Err(failure) => assert failure.message.find("size mismatch") != null
  }
}

test test_source_lock_rejects_size_above_int_max { |ctx|
  let root = test.temp_dir(ctx, name: "source-lock-size-overflow")?
  let lock = fp"{root}/sources.lock.json"
  lock.write("{\"schema_version\":1,\"inputs\":[{\"name\":\"sample\",\"version\":\"1\",\"url\":\"https://example.com/a\",\"sha256\":\"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\",\"size\":9223372036854775808}]}")
  assert locks.load_sources(lock) is Err(_)
}

test test_source_lock_rejects_invalid_inputs { |ctx|
  let root = test.temp_dir(ctx, name: "source-lock-invalid")?
  let lock = fp"{root}/sources.lock.json"
  let digest = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
  for url in ["https://", "https:///missing", "https://[broken", "https://host:invalid/x", "http://example.test"] {
    lock.write(f"{{\"schema_version\":1,\"inputs\":[{{\"name\":\"sample\",\"version\":\"1\",\"url\":\"{url}\",\"sha256\":\"{digest}\"}}]}}")
    assert locks.load_sources(lock) is Err(_), url
  }
  lock.write(f"{{\"schema_version\":1,\"inputs\":[{{\"name\":\"sample\",\"version\":\"1\",\"url\":\"https://example.test\",\"sha256\":\"{digest}\",\"size\":true}}]}}")
  assert locks.load_sources(lock) is Err(_)
}

test test_fetch_reuses_a_verified_file_and_refuses_symlinks { |ctx|
  let root = test.temp_dir(ctx, name: "source-lock-cached")?
  let lock = fp"{root}/sources.lock.json"
  let input = fp"{root}/input"
  let link = fp"{root}/link"
  input.write("hello")
  lock.write("{\"schema_version\":1,\"inputs\":[{\"name\":\"sample\",\"version\":\"1\",\"url\":\"https://example.com/a\",\"sha256\":\"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\",\"size\":5}]}")
  let item = locks.load_sources(lock)?[0]
  assert locks.fetch_input(item, input)? == input
  fs.symlink(input, link)
  assert locks.fetch_input(item, link) is Err(_)
  assert input.read_text()? == "hello"
}

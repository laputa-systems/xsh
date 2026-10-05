proc padded(text: Str, width: Int) [error] -> Result[Bytes, Error] {
  let raw = bytes.from_text(text)
  return Ok(bytes.concat([raw, bytes.zero(width - raw.len())?]))
}

# One 384-byte glibc `struct utmp` record in native byte order.
proc utmp_record(kind: Int, pid: Int, line: Str, id: Str, account: Str, host: Str, sec: Int, addr: List[Int]) [error] -> Result[Bytes, Error] {
  return Ok(bytes.concat([
    bytes.pack_le(kind, 2)?,
    bytes.zero(2)?,
    bytes.pack_le(pid, 4)?,
    padded(line, 32)?,
    padded(id, 4)?,
    padded(account, 32)?,
    padded(host, 256)?,
    bytes.pack_le(3, 2)?,
    bytes.pack_le(4, 2)?,
    bytes.pack_le(77, 4)?,
    bytes.pack_le(sec, 4)?,
    bytes.pack_le(250000, 4)?,
    bytes.from_ints(addr)?,
    bytes.zero(20)?,
  ]))
}

test test_read_utmp_decodes_every_field { |ctx|
  let root = test.temp_dir(ctx, name: "utmp")?
  let file = fp"{root}/utmp"
  let none = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
  let v4 = [192, 0, 2, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
  let v6 = [32, 1, 13, 184, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]
  file.write(bytes.concat([
    utmp_record(2, 0, "~", "~~", "reboot", "6.1.0", 1700000000, none)?,
    utmp_record(7, 1234, "tty1", "1", "alice", "example.org", 1700000100, v4)?,
    utmp_record(8, 99, "pts/3", "ts/3", "", "", 1700000200, none)?,
    utmp_record(6, 5, "pts/4", "ts/4", "LOGIN", "[::1]", 1700000300, v6)?,
  ]))?

  let records = unix.read_utmp(file)?
  assert records.len() == 4

  let boot = records[0]
  assert boot.type == 2
  assert boot.kind == "boot_time"
  assert boot.line == "~" and boot.id == "~~" and boot.user == "reboot"
  assert boot.host == "6.1.0"
  assert boot.time_sec == 1700000000
  assert boot.addr == ""

  let session = records[1]
  assert session.kind == "user_process"
  assert session.pid == 1234
  assert session.line == "tty1" and session.id == "1"
  assert session.user == "alice" and session.host == "example.org"
  assert session.termination == 3 and session.exit_status == 4
  assert session.session == 77
  assert session.time_sec == 1700000100 and session.time_usec == 250000
  assert session.addr == "192.0.2.7"

  assert records[2].kind == "dead_process"
  assert records[2].user == ""
  assert records[3].kind == "login_process"
  assert records[3].addr == "2001:db8::1"
}

test test_read_utmp_ignores_a_partial_trailing_record_and_names_failures { |ctx|
  let root = test.temp_dir(ctx, name: "utmp-edge")?
  let junk = fp"{root}/junk"
  junk.write("hello")
  assert unix.read_utmp(junk)? == []

  let empty = fp"{root}/empty"
  empty.write("")
  assert unix.read_utmp(empty)? == []

  let none = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
  let torn = fp"{root}/torn"
  torn.write(bytes.concat([utmp_record(7, 1, "tty1", "1", "bob", "", 1, none)?, bytes.from_text("tail")]))?
  assert unix.read_utmp(torn)?.len() == 1

  let missing = unix.read_utmp(fp"{root}/missing")
  assert missing is Err(is NotFound)
  if let Err(failure) = missing {
    assert failure.errno == 2
    assert f"{root}/missing" in failure.message
  }
  let directory = unix.read_utmp(root)
  assert directory is Err(_)
  if let Err(failure) = directory {
    assert failure.errno == 21
  }
}

test test_read_utmp_default_file_is_read_or_reported_missing {
  match unix.read_utmp() {
    Ok(records) => assert records.len() >= 0
    Err(failure) => assert failure.errno == 2
  }
}

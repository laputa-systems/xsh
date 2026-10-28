use core.lib.syslog_kmsg as kmsg

proc bound_socket(name: Str) [process, net, error] -> Result[Int] {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_UNIX, c.SOCK_DGRAM.bit_or(c.SOCK_NONBLOCK))?
  linux.bind(fd, {family: "unix", address: f"@{name}"})
  Ok(fd)
}

proc deliver(name: Str, record: Bytes) [process, net, error] {
  let c = linux.net_constants()
  let fd = linux.socket(c.AF_UNIX, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)
  assert linux.sendto(fd, record, {family: "unix", address: f"@{name}"})? == record.len()
}

pure kind_of(outcome: kmsg.Outcome) -> Str {
  match outcome {
    kmsg.Message(_) => "message"
    kmsg.Malformed(_) => "malformed"
    kmsg.Overrun => "overrun"
    kmsg.Empty => "empty"
  }
}

proc failure_of(errno_source: Result[Int, Error]) -> Result[Bytes, Error] {
  match errno_source {
    Ok(_) => Ok(b"")
    Err(failure) => Err(failure)
  }
}

test test_a_record_with_a_dictionary_parses_into_typed_fields {
  let raw = b"6,339,5140900,-;NET: Registered protocol family 10\n SUBSYSTEM=net\n DEVICE=+net:lo\n"
  guard let record = kmsg.parse(raw) else {
    assert false, "the record must parse"
    return
  }
  assert record.priority == 6 and record.facility == 0 and record.severity == 6
  assert record.sequence == 339 and record.microseconds == 5140900 and ! record.continuation
  assert record.message == b"NET: Registered protocol family 10"
  assert record.fields == 2
}

test test_priority_splits_into_facility_and_severity_and_flags_continuations {
  guard let record = kmsg.parse(b"30,7,12,c,caller=T1;daemon text\n") else {
    assert false, "the record must parse"
    return
  }
  assert record.facility == 3 and record.severity == 6 and record.continuation
  assert record.message == b"daemon text" and record.fields == 0
}

test test_malformed_and_oversize_records_do_not_parse {
  for raw in [b"", b"no header", b"6,x,1,-;text", b"6,1,2;text", b",,,;text"] {
    assert kmsg.parse(raw) == null
  }
  assert kmsg.parse(bytes.zero(kmsg.RECORD_LIMIT + 1)?) == null
}

test test_a_message_with_arbitrary_bytes_is_kept_byte_for_byte {
  guard let record = kmsg.parse(b"6,1,2,-;\xff\x00binary\\x41\n") else {
    assert false, "the record must parse"
    return
  }
  assert record.message == b"\xff\x00binary\\x41"
}

test test_each_read_of_the_device_returns_one_whole_record_and_an_empty_queue_ends_the_drain {
  let fd = bound_socket(f"kmsg-drain-{process.current_pid()?}")?
  defer unix.close_fd(fd)
  let name = f"kmsg-drain-{process.current_pid()?}"
  deliver(name, b"6,1,10,-;first\n")
  deliver(name, b"6,2,20,-;second\n SUBSYSTEM=x\n")
  deliver(name, b"garbage without a header")
  let drained = kmsg.drain({read: kmsg.device_read}, fd, 100)?
  assert drained.records.len() == 2 and drained.malformed == 1 and drained.overruns == 0
  assert drained.records[0].sequence == 1 and drained.records[1].sequence == 2
  assert drained.records[1].fields == 1
  assert kmsg.drain({read: kmsg.device_read}, fd, 100)?.records.is_empty()
}

test test_the_drain_stops_at_its_limit_and_leaves_the_rest_queued {
  let name = f"kmsg-limit-{process.current_pid()?}"
  let fd = bound_socket(name)?
  defer unix.close_fd(fd)
  for index in range(5) { deliver(name, bytes.from_text(f"6,{index},1,-;m{index}\n")) }
  assert kmsg.drain({read: kmsg.device_read}, fd, 3)?.records.len() == 3
  assert kmsg.drain({read: kmsg.device_read}, fd, 10)?.records.len() == 2
}

test test_epipe_is_an_overrun_eagain_is_empty_and_other_errors_are_reported {
  match kmsg.classify(failure_of(unix.write_fd(2147483647, b"x"))) {
    Ok(_) => assert false, "EBADF must not be swallowed"
    Err(failure) => assert failure.errno == 9
  }
  let empty = bound_socket(f"kmsg-empty-{process.current_pid()?}")?
  defer unix.close_fd(empty)
  assert kind_of(kmsg.classify(kmsg.device_read(empty))?) == "empty"
  let c = linux.net_constants()
  let client = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
  defer unix.close_fd(client)
  let nothing = linux.socket(c.AF_UNIX, c.SOCK_STREAM)?
  linux.bind(nothing, {family: "unix", address: f"@kmsg-broken-{process.current_pid()?}"})
  linux.listen(nothing)
  linux.connect(client, linux.getsockname(nothing)?)
  unix.close_fd(nothing)
  let broken = unix.write_fd(client, b"x")
  match kmsg.classify(failure_of(broken)) {
    Ok(outcome) => assert kind_of(outcome) == "overrun"
    Err(failure) => assert false, failure.message
  }
  assert kind_of(kmsg.classify(Ok(b"6,1,2,-;ok"))?) == "message"
  assert kind_of(kmsg.classify(Ok(b"junk"))?) == "malformed"
}

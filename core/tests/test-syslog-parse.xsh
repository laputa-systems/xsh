use core.lib.syslog_parse as parse

proc datagram(text: Str) -> Bytes {
  bytes.from_text(text)
}

pure repeat(text: Str, count: Int) -> Str {
  [text for _ in range(count)].join("")
}

test test_every_valid_priority_splits_into_facility_and_severity {
  for value in range(192) {
    let parsed = parse.parse(datagram(f"<{value}>hello"))
    assert parsed.priority == .Valid, f"{value}"
    assert parsed.facility == value / 8 and parsed.severity == value % 8, f"{value}"
    assert parsed.content == b"hello"
  }
  assert parse.facility_name(0) == "kern" and parse.facility_name(23) == "local7"
  assert parse.facility_name(24) == "unknown" and parse.facility_name(-1) == "unknown"
  assert parse.severity_name(0) == "emerg" and parse.severity_name(7) == "debug"
  assert parse.severity_name(8) == "unknown"
}

test test_out_of_range_and_noncanonical_priorities_are_invalid_and_keep_their_bytes {
  for text in ["<192>x", "<999>x", "<01>x", "<007>x", "<00>x", "<256>x"] {
    let parsed = parse.parse(datagram(text))
    assert parsed.priority == .Invalid, text
    assert parsed.facility == parse.DEFAULT_FACILITY and parsed.severity == parse.DEFAULT_SEVERITY
    assert parsed.content == datagram(text), text
    assert parsed.tag == null and parsed.host == null and parsed.header_time == null
  }
}

test test_missing_priorities_keep_the_whole_datagram {
  for text in ["", "hello", "<>x", "<a>x", "<13", "13>x", " <13>x", "<1234>x", "<13 >x"] {
    let parsed = parse.parse(datagram(text))
    assert parsed.priority == .Missing or parsed.priority == .Invalid, text
    assert parsed.content == datagram(text), text
  }
  assert parse.parse(datagram("hello")).priority == .Missing
  assert parse.parse(datagram("<1234>x")).priority == .Missing
}

test test_libc_header_without_host_yields_tag_and_pid {
  let with_pid = parse.parse(datagram("<13>Oct 28 10:11:12 sshd[4242]: Accepted key"))
  assert with_pid.header_time == "Oct 28 10:11:12" and with_pid.host == null
  assert with_pid.tag == "sshd" and with_pid.tag_pid == 4242
  assert with_pid.content == b"Accepted key"
  let without_pid = parse.parse(datagram("<38>Dec  5 01:02:03 su: 'su root' failed"))
  assert without_pid.header_time == "Dec  5 01:02:03" and without_pid.tag == "su" and without_pid.tag_pid == null
  assert without_pid.facility == 4 and without_pid.severity == 6
  assert without_pid.content == b"'su root' failed"
}

test test_rfc3164_header_with_host_yields_host_tag_and_pid {
  let parsed = parse.parse(datagram("<34>Oct 11 22:14:15 mymachine su[77]: 'su root' failed on /dev/pts/8"))
  assert parsed.host == "mymachine" and parsed.tag == "su" and parsed.tag_pid == 77
  assert parsed.header_time == "Oct 11 22:14:15"
  assert parsed.content == b"'su root' failed on /dev/pts/8"
}

test test_header_without_timestamp_may_still_carry_a_tag {
  let parsed = parse.parse(datagram("<13>myapp: hello"))
  assert parsed.tag == "myapp" and parsed.header_time == null and parsed.host == null
  assert parsed.content == b"hello"
  let plain = parse.parse(datagram("<13>just some words"))
  assert plain.tag == null and plain.content == b"just some words" and plain.priority == .Valid
}

test test_malformed_timestamps_leave_the_payload_unparsed {
  for text in [
    "Foo 28 10:11:12 host tag: x", "Oct 32 10:11:12 host tag: x", "Oct 28 25:11:12 host tag: x",
    "Oct 28 10:61:12 host tag: x", "Oct 28 10:11:61 host tag: x", "Oct 28 1:11:12 host tag: x",
    "oct 28 10:11:12 host tag: x",
  ] {
    let parsed = parse.parse(datagram(f"<13>{text}"))
    assert parsed.header_time == null and parsed.tag == null and parsed.host == null, text
    assert parsed.content == datagram(text), text
  }
}

test test_tag_pid_and_host_limits_are_exact {
  let tag48 = repeat("t", 48)
  let tag49 = repeat("t", 49)
  assert parse.parse(datagram(f"<13>Oct 28 10:11:12 {tag48}: x")).tag == tag48
  assert parse.parse(datagram(f"<13>Oct 28 10:11:12 {tag49}: x")).tag == null
  assert parse.parse(datagram("<13>Oct 28 10:11:12 tag[1234567890]: x")).tag_pid == 1234567890
  assert parse.parse(datagram("<13>Oct 28 10:11:12 tag[12345678901]: x")).tag == null
  let host255 = repeat("h", 255)
  let host256 = repeat("h", 256)
  assert parse.parse(datagram(f"<13>Oct 28 10:11:12 {host255} tag: x")).host == host255
  assert parse.parse(datagram(f"<13>Oct 28 10:11:12 {host256} tag: x")).host == null
  assert parse.parse(datagram("<13>Oct 28 10:11:12 tag[]: x")).tag == null
  assert parse.parse(datagram("<13>Oct 28 10:11:12 tag[abc]: x")).tag == null
}

test test_trailing_nul_and_newline_are_dropped_but_embedded_bytes_are_kept {
  assert parse.parse(datagram("<13>tag: hi\n")).content == b"hi"
  assert parse.parse(bytes.concat([datagram("<13>tag: hi"), b"\0"])).content == b"hi"
  assert parse.parse(bytes.concat([datagram("<13>tag: hi"), b"\0\0\n"])).content == b"hi\0\0"
  assert parse.parse(bytes.concat([datagram("<13>tag: hi"), bytes.zero(8)?])).content == b"hi"
  assert parse.parse(bytes.concat([datagram("<13>tag: hi"), bytes.zero(9)?])).content == bytes.concat([b"hi", b"\0"])
  assert parse.parse(b"<13>tag: a\0b\nc").content == b"a\0b\nc"
}

test test_non_utf8_payloads_keep_their_bytes_and_still_yield_the_header_when_it_is_text {
  let binary = bytes.concat([datagram("<13>Oct 28 10:11:12 tag: "), b"\xff\xfe\x00\x80"])
  let parsed = parse.parse(binary)
  assert parsed.tag == "tag" and parsed.header_time == "Oct 28 10:11:12"
  assert parsed.content == b"\xff\xfe\x00\x80"
  let early = parse.parse(b"<14>\xffbinary")
  assert early.priority == .Valid and early.severity == 6 and early.content == b"\xffbinary"
}

test test_a_priority_inside_the_payload_is_not_reparsed {
  let parsed = parse.parse(datagram("<13>tag: <191>nested"))
  assert parsed.facility == 1 and parsed.severity == 5 and parsed.content == b"<191>nested"
}

test test_hostile_large_inputs_finish_with_all_bytes_accounted_for {
  let started = time.monotonic()?
  let shapes: List[Bytes] = [
    datagram(repeat("<", 8192)),
    datagram(repeat(" ", 8192)),
    datagram("<13>" + repeat("a", 8192)),
    datagram("<13>Oct 28 10:11:12 " + repeat("h", 8000) + " tag: x"),
    datagram("<13>" + repeat("tag[", 2000)),
    bytes.zero(8192)?,
    datagram("<13>tag:" + repeat(" ", 8000)),
  ]
  for shape in shapes {
    let parsed = parse.parse(shape)
    assert parsed.content.len() <= shape.len()
    assert parsed.content.len() + 5 + parse.HEADER_WINDOW >= shape.len() - parse.MAX_TRAILING_NULS - 1
  }
  assert time.monotonic()? - started < 3000000000
}

test test_the_parser_relies_on_the_utf8_failure_naming_the_first_bad_byte {
  match b"ab\xffc".utf8() {
    Err(failure) => assert failure.message.ends_with("at byte 2"), failure.message
    Ok(_) => assert false, "invalid UTF-8 decoded"
  }
}

test test_a_window_cut_inside_a_multibyte_character_still_parses_the_header {
  let euro = repeat("\u{20ac}", 300)
  let parsed = parse.parse(datagram(f"<13>Oct 28 10:11:12 host tag[9]: {euro}"))
  assert parsed.tag == "tag" and parsed.host == "host" and parsed.tag_pid == 9
  assert parsed.content == datagram(euro)
}

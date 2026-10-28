use core.lib.syslog_parse as parse
use core.lib.syslog_render as render

const LINE = rx"^seq=[0-9]+ boot=\x22[0-9a-f-]+\x22 boottime_ns=[0-9]+ pid=(?:[0-9]+|-) uid=(?:[0-9]+|-) gid=(?:[0-9]+|-) source=(?:socket|kmsg) kseq=(?:[0-9]+|-) ktime_us=(?:[0-9]+|-) facility=[a-z0-9]+ severity=[a-z]+ priority=(?:valid|missing|invalid) tag=(?:-|\x22[A-Za-z0-9_./-]*\x22) tag_pid=(?:[0-9]+|-) host=(?:-|\x22[A-Za-z0-9_.:-]*\x22) header_time=(?:-|\x22[A-Za-z]{3} [ 0-9][0-9] [0-9:]{8}\x22) cut=[0-9]+ (?:msg=\x22(?:[^\x22\\]|\\[nrt\x22\\]|\\x[0-9a-f]{2}|\\u\{[0-9a-f]{4}\})*\x22|payload_base64=\x22[A-Za-z0-9+/=]*\x22)$"
const PRINTABLE_ONLY = rx"^[\u{20}-\u{7e}\u{a0}-\u{2027}\u{202f}-\u{2065}\u{206a}-\u{10ffff}]*$"

proc entry(data: Bytes) -> render.Entry {
  render.Entry(sequence: 1, source: "socket", kernel_sequence: null, kernel_microseconds: null, boot_id: "0123abcd-4567-89ef-0123-456789abcdef", boottime_ns: 99, pid: 41, uid: 1000, gid: 100, message: parse.parse(data))
}

proc line(data: Bytes) -> Str {
  render.render(entry(data))
}

pure repeat(text: Str, count: Int) -> Str {
  [text for _ in range(count)].join("")
}

proc assert_one_well_formed_line(rendered: Str, label: Str) {
  assert LINE.matches(rendered), f"{label}: {rendered}"
  assert rendered.lines().len() == 1, label
  assert PRINTABLE_ONLY.matches(rendered), label
}

test test_a_plain_message_renders_every_field_in_order {
  assert line(bytes.from_text("<13>Oct 28 10:11:12 host sshd[4242]: Accepted key")) == "seq=1 boot=\"0123abcd-4567-89ef-0123-456789abcdef\" boottime_ns=99 pid=41 uid=1000 gid=100 source=socket kseq=- ktime_us=- facility=user severity=notice priority=valid tag=\"sshd\" tag_pid=4242 host=\"host\" header_time=\"Oct 28 10:11:12\" cut=0 msg=\"Accepted key\""
}

test test_absent_fields_render_as_a_bare_dash {
  let rendered = render.render(render.Entry(sequence: 7, source: "socket", kernel_sequence: null, kernel_microseconds: null, boot_id: "b", boottime_ns: 1, pid: null, uid: null, gid: null, message: parse.parse(b"hi")))
  assert " pid=- uid=- gid=- " in rendered
  assert " tag=- tag_pid=- host=- header_time=- " in rendered
  assert rendered.ends_with(" priority=missing tag=- tag_pid=- host=- header_time=- cut=0 msg=\"hi\"")
}

test test_quote_backslash_newline_and_controls_are_escaped {
  let rendered = line(bytes.from_text("<13>tag: a\"b\\c\nd\re\tf\u{1}g\u{7f}h\0i"))
  assert rendered.ends_with("msg=\"a\\\"b\\\\c\\nd\\re\\tf\\x01g\\x7fh\\x00i\"")
  assert_one_well_formed_line(rendered, "controls")
}

test test_unicode_line_separators_and_bidirectional_controls_are_escaped {
  let rendered = line(bytes.from_text("<13>x \u{85} \u{2028} \u{2029} \u{202e} \u{2066} \u{200f} end \u{e9} \u{1f600}"))
  assert rendered.ends_with("msg=\"x \\u{0085} \\u{2028} \\u{2029} \\u{202e} \\u{2066} \\u{200f} end \u{e9} \u{1f600}\"")
  assert_one_well_formed_line(rendered, "unicode")
}

test test_forged_record_text_stays_inside_the_escaped_message_field {
  let forged = "x\nseq=999 uid=0 gid=0 facility=kern severity=emerg priority=valid tag=\"login\" msg=\"root logged in\"\r\n"
  let rendered = line(bytes.from_text(f"<13>Oct 28 10:11:12 host evil[1]: {forged}"))
  assert_one_well_formed_line(rendered, "forged")
  assert rendered.starts_with("seq=1 boot=")
  assert " uid=1000 gid=100 " in rendered
  assert "tag=\"evil\"" in rendered
  assert "\n" not in rendered and "\r" not in rendered
  assert "\\nseq=999 uid=0" in rendered
  let boundary = rendered.find(" msg=\"") ?? -1
  assert boundary > 0
  assert "seq=999" not in rendered.byte_slice(0, boundary)
}

test test_a_payload_that_is_not_utf8_renders_as_base64_in_its_own_field {
  let original = b"\xff\xfe\0binary\x80"
  let rendered = line(bytes.concat([b"<13>tag: ", original]))
  assert_one_well_formed_line(rendered, "binary")
  assert " payload_base64=\"" in rendered
  assert " msg=" not in rendered
  let encoded = rx"payload_base64=\x22([A-Za-z0-9+/=]*)\x22$".captures(rendered)[1]
  assert encoded.base64_decode()? == original
}

test test_a_payload_with_too_many_characters_to_escape_renders_as_base64 {
  let at_limit = repeat("\n", render.ESCAPE_LIMIT)
  let over_limit = repeat("\n", render.ESCAPE_LIMIT + 1)
  assert " msg=\"" in line(bytes.from_text(f"<13>tag: {at_limit}."))
  let over = line(bytes.from_text(f"<13>tag: {over_limit}."))
  assert " payload_base64=\"" in over
  assert_one_well_formed_line(over, "over limit")
}

test test_payload_is_cut_at_the_limit_and_the_count_is_reported {
  let rendered = line(bytes.from_text(f"<13>tag: {repeat("a", 5000)}"))
  assert " cut=2952 " in rendered
  assert f"msg=\"{repeat("a", render.CONTENT_LIMIT)}\"" in rendered
  let exact = line(bytes.from_text(f"<13>tag: {repeat("a", render.CONTENT_LIMIT)}"))
  assert " cut=0 " in exact
}

test test_a_cut_inside_a_multibyte_character_backs_up_to_its_start {
  let rendered = line(bytes.from_text(f"<13>tag: {repeat("\u{20ac}", 1000)}"))
  assert " cut=954 " in rendered
  assert f"msg=\"{repeat("\u{20ac}", 682)}\"" in rendered
  assert_one_well_formed_line(rendered, "multibyte cut")
}

test test_every_single_byte_datagram_renders_one_well_formed_line {
  for value in range(256) {
    let rendered = line(bytes.from_ints([value])?)
    assert_one_well_formed_line(rendered, f"byte {value}")
    let framed = line(bytes.concat([b"<13>tag: ", bytes.from_ints([value, value])?]))
    assert_one_well_formed_line(framed, f"framed byte {value}")
  }
}

proc generated(seed: Int, count: Int) -> List[Bytes] {
  const alphabet = [0, 10, 13, 34, 92, 9, 255, 128, 194, 226, 60, 62, 49, 51, 32, 58, 91, 93, 97, 98, 127, 1, 77, 111]
  var state = seed
  var datagrams: List[Bytes] = []
  for _ in range(count) {
    state = (state * 1103515245 + 12345) % 2147483648
    let length = state / 65536 % 200
    var values: List[Int] = []
    for _ in range(length) {
      state = (state * 1103515245 + 12345) % 2147483648
      values += [if state / 16 % 4 == 0 { state / 256 % 256 } else { alphabet[state / 256 % alphabet.len()] }]
    }
    datagrams += [bytes.from_ints(values)?]
  }
  datagrams
}

test test_any_generated_datagram_yields_one_line_and_never_an_error {
  var index = 0
  for data in generated(20261028, 400) {
    assert_one_well_formed_line(line(data), f"generated {index}")
    index += 1
  }
}

test test_generated_binary_payloads_survive_the_base64_round_trip {
  for data in generated(7, 100) {
    let payload = bytes.concat([data, b"\xff"])
    let rendered = line(bytes.concat([b"<13>tag: ", payload]))
    let encoded = rx"payload_base64=\x22([A-Za-z0-9+/=]*)\x22$".captures(rendered)
    assert ! encoded.is_empty(), rendered
    assert encoded[1].base64_decode()? == payload
  }
}

test test_every_line_stays_under_the_pipe_atomic_write_size_at_its_worst_case {
  for payload in [repeat("\"", 2048), repeat("\\", 2048), repeat("\n", 2048), repeat("\u{2028}", 700), repeat("a", 2048)] {
    let rendered = line(bytes.from_text(f"<13>Oct 28 10:11:12 {repeat("h", 255)} {repeat("t", 48)}[4294967295]: {payload}."))
    assert rendered.byte_len() < 4096, f"{rendered.byte_len()}"
    assert_one_well_formed_line(rendered, "worst case")
  }
  let wide = line(bytes.concat([bytes.from_text(f"<13>Oct 28 10:11:12 {repeat("h", 255)} {repeat("t", 48)}[4294967295]: "), bytes.zero(2048)?, b"\xff"]))
  assert wide.byte_len() < 4096
}

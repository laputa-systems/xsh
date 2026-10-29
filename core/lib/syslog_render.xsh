##! Renders one parsed syslog datagram as exactly one physical line.
##!
##! Every payload byte reaches the line inside a quoted field, so no datagram can
##! add a field, end the line or forge another record. Valid UTF-8 with few
##! characters to escape keeps its text; any other payload is rendered as base64
##! in a field of its own, so the work per record is bounded by the payload cap.

use syslog_parse as parse

## One record: receipt data and kernel-reported sender credentials, then what the
## sender claimed. `source` is `socket` for a datagram and `kmsg` for a kernel
## message, which also carries the kernel's own sequence number and timestamp.
export type Entry = {
  sequence: Int,
  source: Str,
  kernel_sequence: Int?,
  kernel_microseconds: Int?,
  boot_id: Str,
  boottime_ns: Int,
  pid: Int?,
  uid: Int?,
  gid: Int?,
  message: parse.Parsed,
}

## Payload bytes rendered per record; the rest is counted in the `cut` field.
export const CONTENT_LIMIT = 2048
## Characters that may need an escape before a payload is rendered as base64.
export const ESCAPE_LIMIT = 64
## Bytes an escaped payload may take before it is rendered as base64. With the
## payload cap this keeps every line under PIPE_BUF, so one write per line is atomic.
export const ESCAPED_LIMIT = 3000

type Hit = {start: Int, end: Int, text: Str}

const HEX = "0123456789abcdef"
const UNSAFE = rx"[\x00-\x1f\x7f\u{80}-\u{9f}\u{2028}\u{2029}\u{200e}\u{200f}\u{202a}-\u{202e}\u{2066}-\u{2069}]"

pure hex_digit(value: Int) -> Str {
  HEX.byte_slice(value, 1)
}

pure code_point(character: Str) -> Int {
  let encoded = bytes.from_text(character)
  let first = encoded.byte_at(0) ?? 0
  let second = (encoded.byte_at(1) ?? 0) % 64
  let third = (encoded.byte_at(2) ?? 0) % 64
  if first < 128 { first } else if first < 224 { first % 32 * 64 + second } else { first % 16 * 4096 + second * 64 + third }
}

pure escape_unsafe(character: Str) -> Str {
  let code = code_point(character)
  if code == 10 { return "\\n" }
  if code == 13 { return "\\r" }
  if code == 9 { return "\\t" }
  if code < 128 { return f"\\x{hex_digit(code / 16)}{hex_digit(code % 16)}" }
  f"\\u{{{hex_digit(code / 4096 % 16)}{hex_digit(code / 256 % 16)}{hex_digit(code / 16 % 16)}{hex_digit(code % 16)}}}"
}

pure escape_text(text: Str, hits: List[Hit]) -> Str {
  var pieces: List[Str] = []
  var position = 0
  for hit in hits {
    pieces += [text.byte_slice(position, hit.start - position), escape_unsafe(hit.text)]
    position = hit.end
  }
  pieces += [text.byte_slice(position)]
  pieces.join("")
}

## A payload as the text of a quoted field. Backslash, quote, every control
## character and the Unicode line and paragraph separators and bidirectional
## controls are escaped. Returns null when the payload is not valid UTF-8, has
## more than `ESCAPE_LIMIT` characters to escape or escapes to more than
## `ESCAPED_LIMIT` bytes.
export pure escape(data: Bytes) -> Str? {
  let decoded = data.utf8()
  if decoded is Err(_) { return null }
  let text = decoded ?? ""
  let quoted = text.replace("\\", with: "\\\\").replace("\"", with: "\\\"")
  let hits = UNSAFE.find(quoted)
  if hits.len() > ESCAPE_LIMIT { return null }
  let escaped = if hits.is_empty() { quoted } else { escape_text(quoted, hits) }
  if escaped.byte_len() > ESCAPED_LIMIT { null } else { escaped }
}

pure boundary(content: Bytes, limit: Int) -> Int {
  if content.len() <= limit { return content.len() }
  var end = limit
  var steps = 0
  while end > 0 and steps < 3 and (content.byte_at(end) ?? 0) / 64 == 2 {
    end -= 1
    steps += 1
  }
  end
}

pure number_field(name: Str, value: Int?) -> Str {
  if value == null { return f"{name}=-" }
  f"{name}={value ?? 0}"
}

pure text_field(name: Str, value: Str?) -> Str {
  if value == null { return f"{name}=-" }
  let raw = bytes.from_text(value ?? "")
  f"{name}=\"{escape(raw) ?? raw.base64()}\""
}

pure payload_field(content: Bytes) -> Str {
  let escaped = escape(content)
  if escaped == null { return f"payload_base64=\"{content.base64()}\"" }
  f"msg=\"{escaped ?? ""}\""
}

## Renders `entry` as one line without a terminator. `cut` counts payload bytes
## beyond `CONTENT_LIMIT`; a cut inside a UTF-8 sequence backs up to its start.
export pure render(entry: Entry) -> Str {
  let message = entry.message
  let kept = boundary(message.content, CONTENT_LIMIT)
  let priority = match message.priority {
    parse.Valid => "valid"
    parse.Missing => "missing"
    parse.Invalid => "invalid"
  }
  [
    f"seq={entry.sequence}",
    text_field("boot", entry.boot_id),
    f"boottime_ns={entry.boottime_ns}",
    number_field("pid", entry.pid),
    number_field("uid", entry.uid),
    number_field("gid", entry.gid),
    f"source={entry.source}",
    number_field("kseq", entry.kernel_sequence),
    number_field("ktime_us", entry.kernel_microseconds),
    f"facility={parse.facility_name(message.facility)}",
    f"severity={parse.severity_name(message.severity)}",
    f"priority={priority}",
    text_field("tag", message.tag),
    number_field("tag_pid", message.tag_pid),
    text_field("host", message.host),
    text_field("header_time", message.header_time),
    f"cut={message.content.len() - kept}",
    payload_field(message.content.slice(0, kept)),
  ].join(" ")
}

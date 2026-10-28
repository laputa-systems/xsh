##! Parses one syslog datagram into typed fields without ever failing.
##!
##! Every byte string yields a record: a missing or invalid priority, an absent or
##! malformed header and a payload that is not text all keep their bytes in
##! `content`. Only the first `HEADER_WINDOW` bytes are read as header text, with
##! linear-time patterns, so the work is a constant plus one slice of the input.

## Whether the datagram carried a priority and whether it was a valid one.
export enum Priority { Valid, Missing, Invalid }

## The fields of one datagram. `facility` and `severity` hold the default user
## notice priority unless `priority` is `Valid`. `host`, `tag`, `tag_pid` and
## `header_time` are what the sender claimed and are never trusted identity.
export type Parsed = {
  priority: Priority,
  facility: Int,
  severity: Int,
  header_time: Str?,
  host: Str?,
  tag: Str?,
  tag_pid: Int?,
  content: Bytes,
}

## Bytes after the priority that are read as header text.
export const HEADER_WINDOW = 400
## The largest valid priority: facility 23, severity 7.
export const MAX_PRIORITY = 191
## Facility of a datagram without a valid priority (user).
export const DEFAULT_FACILITY = 1
## Severity of a datagram without a valid priority (notice).
export const DEFAULT_SEVERITY = 5
## NUL bytes removed from the end of a datagram; a longer run stays in the content.
export const MAX_TRAILING_NULS = 8

const FACILITIES = [
  "kern", "user", "mail", "daemon", "auth", "syslog", "lpr", "news",
  "uucp", "cron", "authpriv", "ftp", "ntp", "audit", "alert", "clock",
  "local0", "local1", "local2", "local3", "local4", "local5", "local6", "local7",
]
const SEVERITIES = ["emerg", "alert", "crit", "err", "warning", "notice", "info", "debug"]

const INVALID_AT = rx"at byte ([0-9]+)$"
const ANY_PRIORITY = rx"^<[0-9]{1,3}>"
const VALID_PRIORITY = rx"^<(0|[1-9][0-9]?|1[0-8][0-9]|19[01])>"
const TIMESTAMP = rx"^((?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) (?: [1-9]|0[1-9]|[12][0-9]|3[01]) (?:[01][0-9]|2[0-3]):[0-5][0-9]:(?:[0-5][0-9]|60)) "
const TAG = rx"^([A-Za-z0-9_./-]{1,48})(?:\[([0-9]{1,10})\])?:(?: |$)"
const HOST_TAG = rx"^([A-Za-z0-9_.:-]{1,255}) ([A-Za-z0-9_./-]{1,48})(?:\[([0-9]{1,10})\])?:(?: |$)"

type Header = {header_time: Str?, host: Str?, tag: Str?, tag_pid: Int?, consumed: Int}

## The name of a facility number, or `unknown`.
export pure facility_name(facility: Int) -> Str {
  if facility >= 0 and facility < FACILITIES.len() { FACILITIES[facility] } else { "unknown" }
}

## The name of a severity number, or `unknown`.
export pure severity_name(severity: Int) -> Str {
  if severity >= 0 and severity < SEVERITIES.len() { SEVERITIES[severity] } else { "unknown" }
}

pure without_trailer(data: Bytes) -> Bytes {
  var end = data.len()
  var nuls = 0
  while end > 0 and nuls < MAX_TRAILING_NULS and (data.byte_at(end - 1) ?? 1) == 0 {
    end -= 1
    nuls += 1
  }
  if end > 0 and (data.byte_at(end - 1) ?? 0) == 10 { end -= 1 }
  data.slice(0, end)
}

pure optional_number(text: Str) -> Int? {
  if text.is_empty() { return null }
  match text.parse_uint() {
    Ok(number) => number
    Err(_) => null
  }
}

pure window_text(body: Bytes, offset: Int) -> Str {
  let remaining = body.len() - offset
  let length = if remaining < HEADER_WINDOW { remaining } else { HEADER_WINDOW }
  let window = body.slice(offset, length)
  match window.utf8() {
    Ok(text) => text
    Err(failure) => {
      let position = INVALID_AT.captures(failure.message)
      if position.is_empty() { return "" }
      window.slice(0, optional_number(position[1]) ?? 0).utf8() ?? ""
    }
  }
}

pure match_end(pattern: Regex, text: Str) -> Int {
  let hits = pattern.find(text)
  if hits.is_empty() { 0 } else { hits[0].end }
}

pure bare_header(consumed: Int) -> Header {
  {header_time: null, host: null, tag: null, tag_pid: null, consumed: consumed}
}

pure tagged(text: Str, stamp: Str?, consumed: Int) -> Header {
  let rest = text.byte_slice(consumed)
  let direct = TAG.captures(rest)
  if ! direct.is_empty() {
    return {header_time: stamp, host: null, tag: direct[1], tag_pid: optional_number(direct[2]), consumed: consumed + match_end(TAG, rest)}
  }
  if stamp == null { return bare_header(0) }
  let hosted = HOST_TAG.captures(rest)
  if hosted.is_empty() { return {header_time: stamp, host: null, tag: null, tag_pid: null, consumed: consumed} }
  {header_time: stamp, host: hosted[1], tag: hosted[2], tag_pid: optional_number(hosted[3]), consumed: consumed + match_end(HOST_TAG, rest)}
}

pure header_of(text: Str) -> Header {
  let stamp = TIMESTAMP.captures(text)
  if stamp.is_empty() { return tagged(text, null, 0) }
  tagged(text, stamp[1], match_end(TIMESTAMP, text))
}

pure unprefixed(priority: Priority, body: Bytes) -> Parsed {
  {priority: priority, facility: DEFAULT_FACILITY, severity: DEFAULT_SEVERITY, header_time: null, host: null, tag: null, tag_pid: null, content: body}
}

## Total over every byte string: never fails and never drops payload bytes other
## than a single trailing newline and up to `MAX_TRAILING_NULS` trailing NULs.
export pure parse(data: Bytes) -> Parsed {
  let body = without_trailer(data)
  let lead = window_text(body, 0).byte_slice(0, 5)
  let value = VALID_PRIORITY.captures(lead)
  if value.is_empty() {
    if ANY_PRIORITY.matches(lead) { return unprefixed(.Invalid, body) }
    return unprefixed(.Missing, body)
  }
  let priority = value[1].parse_uint() ?? 0
  let prefix = match_end(VALID_PRIORITY, lead)
  let header = header_of(window_text(body, prefix))
  {
    priority: .Valid,
    facility: priority / 8,
    severity: priority % 8,
    header_time: header.header_time,
    host: header.host,
    tag: header.tag,
    tag_pid: header.tag_pid,
    content: body.slice(prefix + header.consumed),
  }
}

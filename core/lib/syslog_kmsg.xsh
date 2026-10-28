##! Reads kernel messages from /dev/kmsg as typed records.
##!
##! The device returns one whole record per read and never merges two. A read
##! needs a buffer of at least 8192 bytes, which is also the largest record, so
##! no record is ever cut. A reader that falls behind the ring buffer gets
##! `EPIPE` once and then continues at the oldest record still held: the
##! sequence numbers jump, and `drain` reports the jump as an overrun rather
##! than hiding it.

## The largest record the kernel returns and the read size that fits it.
export const RECORD_LIMIT = 8192
const EAGAIN = 11
const EPIPE = 32

## One kernel message. `facility` and `severity` come from the record's priority
## field; `microseconds` is the kernel timestamp since boot.
export type KernelRecord = {
  priority: Int,
  facility: Int,
  severity: Int,
  sequence: Int,
  microseconds: Int,
  continuation: Bool,
  message: Bytes,
  fields: Int,
}

## What one read of the device produced.
export enum Outcome { Message(KernelRecord), Malformed(Bytes), Overrun, Empty }

## Records read in one drain, the sequence numbers skipped by overruns, and the
## records that could not be parsed.
export type Drained = {records: List[KernelRecord], overruns: Int, malformed: Int}

## The reads a drain makes, so a test can supply records the device would.
export type Source = {read: proc(fd: Int) [process, net, error] -> Result[Bytes, Error]}

const INVALID_AT = rx"at byte ([0-9]+)$"
const HEADER_WINDOW = 96
const HEADER = rx"^([0-9]+),([0-9]+),([0-9]+),([-cf])(?:,[^;]*)?;"

## Reads the device itself with one `read(2)` of `RECORD_LIMIT` bytes.
export proc device_read(fd: Int) [process, net, error] -> Result[Bytes, Error] {
  unix.read_fd(fd, RECORD_LIMIT)
}

pure number(text: Str) -> Int {
  text.parse_uint() ?? 0
}

pure header_text(body: Bytes) -> Str {
  let window = body.slice(0, if body.len() < HEADER_WINDOW { body.len() } else { HEADER_WINDOW })
  match window.utf8() {
    Ok(text) => text
    Err(failure) => {
      let position = INVALID_AT.captures(failure.message)
      if position.is_empty() { return "" }
      window.slice(0, number(position[1])).utf8() ?? ""
    }
  }
}

## Parses one record: `priority,sequence,microseconds,flag[,...];message` and
## optional ` KEY=value` continuation lines. Returns null for anything else.
export pure parse(raw: Bytes) -> KernelRecord? {
  if raw.len() == 0 or raw.len() > RECORD_LIMIT { return null }
  let end = if (raw.byte_at(raw.len() - 1) ?? 0) == 10 { raw.len() - 1 } else { raw.len() }
  let body = raw.slice(0, end)
  let head = header_text(body)
  let found = HEADER.captures(head)
  if found.is_empty() { return null }
  let prefix = found[0].byte_len()
  let rest = body.slice(prefix)
  var message = rest
  var fields = 0
  let lines = rest.lines()
  if lines.len() > 1 {
    message = lines[0]
    fields = lines.len() - 1
  }
  let priority = number(found[1])
  {
    priority,
    facility: priority / 8,
    severity: priority % 8,
    sequence: number(found[2]),
    microseconds: number(found[3]),
    continuation: found[4] != "-",
    message,
    fields,
  }
}

## Classifies one read: a record, an unparseable record, an overrun (`EPIPE`), an
## empty queue (`EAGAIN`), or an error to report.
export pure classify(result: Result[Bytes, Error]) -> Result[Outcome, Error] {
  match result {
    Ok(raw) => {
      if let record = parse(raw) { Ok(.Message(record)) } else { Ok(.Malformed(raw)) }
    }
    Err(failure) => {
      if failure.errno == EPIPE { return Ok(.Overrun) }
      if failure.errno == EAGAIN { return Ok(.Empty) }
      Err(failure)
    }
  }
}

## Reads up to `limit` records without blocking. Any error other than overrun and
## an empty queue is returned, never swallowed.
export proc drain(source: Source, fd: Int, limit: Int) [process, net, error] -> Result[Drained, Error] {
  var records: List[KernelRecord] = []
  var overruns = 0
  var malformed = 0
  var count = 0
  while count < limit {
    count += 1
    match classify(source.read(fd))? {
      Message(record) => { records = records.push(record) }
      Malformed(_) => { malformed += 1 }
      Overrun => { overruns += 1 }
      Empty => { break }
    }
  }
  Ok({records, overruns, malformed})
}

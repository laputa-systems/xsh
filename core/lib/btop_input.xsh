##! Incrementally decodes terminal input bytes into typed, replayable events.
##!
##! The decoder owns only bounded parser state. Paste text is emitted as one
##! inert event, so a caller cannot mistake pasted keys for individual input.

## Maximum bytes retained while an escape sequence is being decoded.
export const MAX_SEQUENCE_BYTES = 16
## Maximum paste payload retained before the paste event is marked truncated.
export const MAX_PASTE_BYTES = 1024
## Delay after Escape before a lone Escape key is emitted.
export const ESCAPE_TIMEOUT_MS = 35

const PASTE_START = b"\x1b[200~"
const PASTE_END = b"\x1b[201~"
const REPLACEMENT = "\u{fffd}"

## A decoded key; text is one valid Unicode scalar and controls stay explicit.
export enum Key {
  Text(Str), Control(Int), Enter, Escape, Backspace, Tab, Cancel,
  Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Delete,
  Function(Int),
}

## Paste stays grouped so its contents cannot dispatch individual hotkeys.
export enum Event { Pressed(Key), Paste(Str, Bool), Eof }

## Pending bytes and bounded paste state for an incremental decode.
export type State = {
  pending: Bytes,
  escape_deadline_ms: Int?,
  discarding_sequence: Bool,
  in_paste: Bool,
  paste_text: Bytes,
  paste_probe: Bytes,
  paste_truncated: Bool,
}

## State and all events produced by one feed, timeout, or finish operation.
export type Step = {state: State, events: List[Event]}

## Start with no pending input.
export pure initial() -> State {
  {
    pending: b"",
    escape_deadline_ms: null,
    discarding_sequence: false,
    in_paste: false,
    paste_text: b"",
    paste_probe: b"",
    paste_truncated: false,
  }
}

pure one_byte(value: Int) -> Bytes {
  bytes.from_ints([value]) ?? b""
}

pure replacement() -> Event {
  Pressed(Text(REPLACEMENT))
}

pure utf8_length(lead: Int) -> Int {
  return 2 when lead >= 194 and lead <= 223
  return 3 when lead >= 224 and lead <= 239
  return 4 when lead >= 240 and lead <= 244
  0
}

# Reject impossible prefixes early so a later chunk cannot pin malformed data.
pure valid_utf8_prefix(data: Bytes, expected: Int) -> Bool {
  let lead = data.byte_at(0) ?? 0
  for offset in range(1, data.len()) {
    let byte = data.byte_at(offset) ?? 0
    return false when byte < 128 or byte > 191
    return false when offset == 1 and lead == 224 and byte < 160
    return false when offset == 1 and lead == 237 and byte > 159
    return false when offset == 1 and lead == 240 and byte < 144
    return false when offset == 1 and lead == 244 and byte > 143
  }
  data.len() <= expected
}

pure key_from_csi(sequence: Bytes) -> Key? {
  match sequence {
    b"\x1b[A" => Up
    b"\x1b[B" => Down
    b"\x1b[C" => Right
    b"\x1b[D" => Left
    b"\x1b[H" => Home
    b"\x1b[F" => End
    b"\x1b[1~" => Home
    b"\x1b[2~" => Insert
    b"\x1b[3~" => Delete
    b"\x1b[4~" => End
    b"\x1b[5~" => PageUp
    b"\x1b[6~" => PageDown
    b"\x1b[15~" => Function(5)
    b"\x1b[17~" => Function(6)
    b"\x1b[18~" => Function(7)
    b"\x1b[19~" => Function(8)
    b"\x1b[20~" => Function(9)
    b"\x1b[21~" => Function(10)
    b"\x1b[23~" => Function(11)
    b"\x1b[24~" => Function(12)
    else => null
  }
}

pure key_from_ss3(sequence: Bytes) -> Key? {
  match sequence {
    b"\x1bOA" => Up
    b"\x1bOB" => Down
    b"\x1bOC" => Right
    b"\x1bOD" => Left
    b"\x1bOH" => Home
    b"\x1bOF" => End
    b"\x1bOP" => Function(1)
    b"\x1bOQ" => Function(2)
    b"\x1bOR" => Function(3)
    b"\x1bOS" => Function(4)
    else => null
  }
}

pure control_key(byte: Int) -> Key? {
  return Enter when byte == 10 or byte == 13
  return Backspace when byte == 8 or byte == 127
  return Tab when byte == 9
  return Cancel when byte == 3
  return null when byte == 26
  Control(byte)
}

pure csi_final_at(data: Bytes) -> Int? {
  for at in range(2, data.len()) {
    let byte = data.byte_at(at) ?? 0
    return at when byte >= 64 and byte <= 126
    return -1 when byte < 32 or byte > 63
  }
  null
}

# Decode all complete units already in pending, leaving only one partial unit.
pure drain(state: State) -> Step {
  var current = state
  var events: List[Event] = []
  var scanning = true
  while scanning and !current.pending.is_empty() and !current.discarding_sequence {
    let data = current.pending
    let first = data.byte_at(0) ?? 0
    if first == 27 {
      if data.len() == 1 { scanning = false; continue }
      let second = data.byte_at(1) ?? 0
      if second == 91 {
        let final_at = csi_final_at(data)
        if final_at == -1 {
          current = {...current, pending: b"", escape_deadline_ms: null}
        } else if final_at == null {
          if data.len() > MAX_SEQUENCE_BYTES {
            current = {...current, pending: b"", discarding_sequence: true}
          } else {
            scanning = false
          }
        } else {
          let sequence = data[..(final_at ?? 0) + 1]
          current = {...current, pending: b"", escape_deadline_ms: null}
          if sequence == PASTE_START {
            current = {...current, in_paste: true, paste_text: b"", paste_probe: b"", paste_truncated: false}
          } else if let key = key_from_csi(sequence) {
            events += [Pressed(key)]
          }
        }
      } else if second == 79 {
        let final_at = csi_final_at(data)
        if final_at == -1 {
          current = {...current, pending: b"", escape_deadline_ms: null}
        } else if final_at == null {
          if data.len() > MAX_SEQUENCE_BYTES {
            current = {...current, pending: b"", discarding_sequence: true}
          } else {
            scanning = false
          }
        } else {
          let sequence = data[..(final_at ?? 0) + 1]
          current = {...current, pending: b"", escape_deadline_ms: null}
          if let key = key_from_ss3(sequence) { events += [Pressed(key)] }
        }
      } else {
        current = {...current, pending: data[1..], escape_deadline_ms: null}
        events += [Pressed(Escape)]
      }
    } else if first < 32 or first == 127 {
      current = {...current, pending: data[1..]}
      if let key = control_key(first) { events += [Pressed(key)] }
    } else if first < 128 {
      current = {...current, pending: data[1..]}
      events += [Pressed(Text(one_byte(first).utf8() ?? ""))]
    } else {
      let size = utf8_length(first)
      if size == 0 {
        current = {...current, pending: data[1..]}
        events += [replacement()]
      } else if data.len() < size and valid_utf8_prefix(data, size) {
        scanning = false
      } else if data.len() < size {
        current = {...current, pending: data[1..]}
        events += [replacement()]
      } else if let Ok(text) = data[..size].utf8() {
        current = {...current, pending: data[size..]}
        events += [Pressed(Text(text))]
      } else {
        current = {...current, pending: data[1..]}
        events += [replacement()]
      }
    }
  }
  {state: current, events: events}
}

pure store_paste_byte(state: State, byte: Int) -> State {
  return {...state, paste_truncated: true} when state.paste_text.len() >= MAX_PASTE_BYTES
  {...state, paste_text: bytes.concat([state.paste_text, one_byte(byte)])}
}

# Match the paste terminator with only a six-byte look-behind buffer.
pure paste_byte(state: State, byte: Int) -> Step {
  var current = {...state, paste_probe: bytes.concat([state.paste_probe, one_byte(byte)])}
  var events: List[Event] = []
  var matching = true
  while matching {
    if current.paste_probe == PASTE_END {
      let text = paste_text(current.paste_text)
      events += [Paste(text, current.paste_truncated)]
      current = {
        ...current,
        in_paste: false,
        paste_text: b"",
        paste_probe: b"",
        paste_truncated: false,
        escape_deadline_ms: null,
      }
      matching = false
    } else if PASTE_END.starts_with(current.paste_probe) {
      matching = false
    } else {
      current = store_paste_byte({...current, paste_probe: current.paste_probe[1..]}, current.paste_probe.byte_at(0) ?? 0)
      if current.paste_probe.is_empty() { matching = false }
    }
  }
  {state: current, events: events}
}

# Decode paste bytes as text, replacing each malformed byte independently.
pure paste_text(data: Bytes) -> Str {
  var text = ""
  var at = 0
  while at < data.len() {
    let lead = data.byte_at(at) ?? 0
    if lead < 128 {
      text += one_byte(lead).utf8() ?? ""
      at += 1
    } else {
      let size = utf8_length(lead)
      if size == 0 or at + size > data.len() {
        text += REPLACEMENT
        at += 1
      } else if !valid_utf8_prefix(data[at..at + size], size) {
        text += REPLACEMENT
        at += 1
      } else if let Ok(character) = data[at..at + size].utf8() {
        text += character
        at += size
      } else {
        text += REPLACEMENT
        at += 1
      }
    }
  }
  text
}

pure expire(state: State, now_ms: Int) -> Step {
  if state.escape_deadline_ms == null or now_ms < (state.escape_deadline_ms ?? 0) {
    return {state: state, events: []}
  }
  if state.pending == b"\x1b" {
    return {
      state: {...state, pending: b"", escape_deadline_ms: null},
      events: [Pressed(Escape)],
    }
  }
  {
    state: {...state, pending: b"", escape_deadline_ms: null, discarding_sequence: false},
    events: [],
  }
}

## Feed any number of bytes; complete events are emitted in input order.
export pure push(state: State, chunk: Bytes, now_ms: Int) -> Step {
  let expired = expire(state, now_ms)
  var current = expired.state
  var events = expired.events
  for at in range(chunk.len()) {
    let byte = chunk.byte_at(at) ?? 0
    if current.in_paste {
      let decoded = paste_byte(current, byte)
      current = decoded.state
      events += decoded.events
    } else if current.discarding_sequence {
      if byte >= 64 and byte <= 126 {
        current = {...current, discarding_sequence: false, escape_deadline_ms: null}
      }
    } else {
      if current.pending.is_empty() and byte == 27 {
        current = {...current, escape_deadline_ms: now_ms + ESCAPE_TIMEOUT_MS}
      }
      current = {...current, pending: bytes.concat([current.pending, one_byte(byte)])}
      let decoded = drain(current)
      current = decoded.state
      if current.pending == b"\x1b" and current.escape_deadline_ms == null {
        current = {...current, escape_deadline_ms: now_ms + ESCAPE_TIMEOUT_MS}
      }
      events += decoded.events
    }
  }
  {state: current, events: events}
}

## Advance a replayable clock; equality with the deadline expires Escape.
export pure timeout(state: State, now_ms: Int) -> Step {
  expire(state, now_ms)
}

## Flush incomplete text safely, discard incomplete controls, then report EOF.
export pure finish(state: State) -> Step {
  var current = state
  var events: List[Event] = []
  if current.in_paste {
    for at in range(current.paste_probe.len()) {
      current = store_paste_byte(current, current.paste_probe.byte_at(at) ?? 0)
    }
    events += [Paste(paste_text(current.paste_text), true)]
  } else if current.pending == b"\x1b" {
    events += [Pressed(Escape)]
  } else if !current.pending.is_empty() and (current.pending.byte_at(0) ?? 0) >= 128 {
    events += [replacement()]
  }
  events += [Eof]
  {state: initial(), events: events}
}

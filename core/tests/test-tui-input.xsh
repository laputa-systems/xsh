use core.lib.tui_input as input

type SplitCase = {sequence: Bytes, expected: input.Event?}

pure split_events(sequence: Bytes, split: Int) -> List[input.Event] {
  let first = input.push(input.initial(), sequence[..split], 100)
  let second = input.push(first.state, sequence[split..], 101)
  first.events + second.events
}

test test_tui_input_ports_fdu_navigation_sequences_across_split_reads {
  let cases: List[SplitCase] = [
    {sequence: b"\x1b[A", expected: .Pressed(input.Up)},
    {sequence: b"\x1bOA", expected: .Pressed(input.Up)},
    {sequence: b"\x1b[5~", expected: .Pressed(input.PageUp)},
    {sequence: b"\x1b[4~", expected: .Pressed(input.End)},
  ]
  for case in cases {
    for split in range(case.sequence.len() + 1) {
      assert split_events(case.sequence, split) == [case.expected ?? input.Eof]
    }
  }
}

test test_tui_input_ports_fdu_incomplete_sequence_survival {
  let now = 100
  let partial = input.push(input.initial(), b"\x1b[1;", now)
  assert partial.events.is_empty()
  assert partial.state.pending == b"\x1b[1;"
  let completed = input.push(partial.state, b"5Dj", now)
  assert completed.events == [input.Pressed(.Text("j"))]
  assert completed.state.pending.is_empty()

  let encoded = bytes.from_text("é")
  let text = input.push(input.initial(), encoded[..1], now)
  assert text.events.is_empty()
  let finished = input.push(text.state, encoded[1..], now)
  assert finished.events == [input.Pressed(.Text("é"))]
}

test test_tui_input_ports_fdu_escape_timeout_and_double_escape {
  let now = 200
  let waiting = input.push(input.initial(), b"\x1b", now)
  assert waiting.events.is_empty()
  assert input.timeout(waiting.state, now + input.ESCAPE_TIMEOUT_MS - 1).events.is_empty()
  let exact = input.timeout(waiting.state, now + input.ESCAPE_TIMEOUT_MS)
  assert exact.events == [input.Pressed(input.Escape)]

  let doubled = input.push(input.initial(), b"\x1b\x1b", now)
  assert doubled.events == [input.Pressed(input.Escape)]
  assert doubled.state.pending == b"\x1b"
  let second = input.timeout(doubled.state, now + input.ESCAPE_TIMEOUT_MS)
  assert second.events == [input.Pressed(input.Escape)]
}

test test_tui_input_unknown_sequences_do_not_become_text {
  let decoded = input.push(input.initial(), b"\x1b[15~x", 300)
  assert decoded.events == [
    input.Pressed(.Function(5)),
    input.Pressed(.Text("x")),
  ]

  let malformed = input.push(input.initial(), b"\xff", 300)
  assert malformed.events == [input.Pressed(.Text("\u{fffd}"))]
}

test test_tui_input_paste_is_inert_and_unconsumed_bytes_hand_back_pending {
  let pasted = input.push(input.initial(), b"\x1b[200~q\rs\x1b[201~", 400)
  assert pasted.events == [input.Paste({text: "q\rs", truncated: false})]

  let partial = input.push(input.initial(), b"\x1b[", 500)
  assert input.unconsumed(partial.state) == b"\x1b["
  assert input.unconsumed(input.initial()) == b""
}

test test_tui_input_escape_deadline_saturates_instead_of_overflowing {
  let latest = 9223372036854775807
  let lone = input.push(input.initial(), b"\x1b", latest)
  assert lone.events.is_empty()
  assert lone.state.escape_deadline_ms == latest
  let expired = input.timeout(lone.state, latest)
  assert expired.events == [input.Pressed(input.Escape)]
  let near = input.push(input.initial(), b"\x1b", latest - 10)
  assert near.state.escape_deadline_ms == latest
  assert input.timeout(near.state, latest - 1).events.is_empty()
}

test test_tui_input_wait_honors_the_pending_escape_deadline {
  assert input.wait_ms(input.initial(), 100, 50) == 50
  let pending = input.push(input.initial(), b"\x1b", 100).state
  assert pending.escape_deadline_ms == 135
  assert input.wait_ms(pending, 100, 50) == 35
  assert input.wait_ms(pending, 120, 50) == 15
  assert input.wait_ms(pending, 120, 10) == 10
  assert input.wait_ms(pending, 134, 50) == 1
}

test test_tui_input_wait_is_one_millisecond_once_the_deadline_has_passed {
  let pending = input.push(input.initial(), b"\x1b", 100).state
  assert input.wait_ms(pending, 135, 50) == 1
  assert input.wait_ms(pending, 9000, 50) == 1
  assert input.wait_ms(pending, 9223372036854775807, 50) == 1
  assert input.wait_ms(input.initial(), 9223372036854775807, 50) == 50
}

test test_tui_input_a_clock_stepped_backward_keeps_the_escape_pending_for_at_most_the_limit {
  let pending = input.push(input.initial(), b"\x1b", 100000).state
  assert input.timeout(pending, 5).events.is_empty()
  assert input.wait_ms(pending, 5, 50) == 50
  assert input.timeout(pending, 100035).events == [input.Pressed(input.Escape)]
}

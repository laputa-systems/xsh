use core.lib.btop_input as input

type SplitCase = {sequence: Bytes, expected: input.Event?}

pure split_events(sequence: Bytes, split: Int) -> List[input.Event] {
  let first = input.push(input.initial(), sequence[..split], 100)
  let second = input.push(first.state, sequence[split..], 101)
  first.events + second.events
}

pure repeated(byte: Int, count: Int) -> Bytes {
  let chunks = collect {
    repeat count times { yield bytes.from_ints([byte]) ?? b"" }
  }
  bytes.concat(chunks)
}

test test_btop_g2_input_split_points_cover_utf8_csi_and_ss3 {
  let cases: List[SplitCase] = [
    {sequence: b"\xce\xbb", expected: .Pressed(.Text("λ"))},
    {sequence: b"\x1b[A", expected: .Pressed(input.Up)},
    {sequence: b"\x1bOQ", expected: .Pressed(.Function(2))},
    {sequence: b"\x1bO1;2A", expected: null},
  ]
  for case in cases {
    for split in range(case.sequence.len() + 1) {
      if case.expected == null {
        assert split_events(case.sequence, split).is_empty()
      } else {
        assert split_events(case.sequence, split) == [case.expected ?? input.Eof]
      }
    }
  }
}

test test_btop_g2_input_emits_multiple_events_from_one_read {
  let decoded = input.push(input.initial(), b"q\r\x1b[A\x1bOP\x1a\x03", 10)

  assert decoded.events == [
    input.Pressed(.Text("q")),
    input.Pressed(input.Enter),
    input.Pressed(input.Up),
    input.Pressed(.Function(1)),
    input.Pressed(input.Cancel),
  ]
  assert decoded.state.pending.is_empty()
}

test test_btop_input_unsupported_mouse_sequences_are_ignored {
  let decoded = input.push(input.initial(), b"\x1b[<0;12;4M\x1b[<0;12;4m", 100)

  assert decoded.events.is_empty()
  assert decoded.state.pending.is_empty()
}

test test_btop_g2_input_escape_timeout_has_replayable_boundary {
  let waiting = input.push(input.initial(), b"\x1b", 100)
  let before = input.timeout(waiting.state, 134)
  let exact = input.timeout(before.state, 135)

  assert before.events.is_empty()
  assert before.state.pending == b"\x1b"
  assert exact.events == [input.Pressed(input.Escape)]
  assert exact.state.pending.is_empty()

  let partial = input.push(input.initial(), b"\x1b[", 200)
  let expired = input.timeout(partial.state, 235)
  assert expired.events.is_empty()
  assert expired.state.pending.is_empty()

  let late = input.push(waiting.state, b"[", 135)
  assert late.events == [
    input.Pressed(input.Escape),
    input.Pressed(.Text("[")),
  ]
}

test test_btop_g2_input_malformed_utf8_replaces_bad_bytes_and_finishes_partial {
  let malformed = input.push(input.initial(), b"\xff\x80A\xc3(", 0)
  assert malformed.events == [
    input.Pressed(.Text("\u{fffd}")),
    input.Pressed(.Text("\u{fffd}")),
    input.Pressed(.Text("A")),
    input.Pressed(.Text("\u{fffd}")),
    input.Pressed(.Text("(")),
  ]

  let incomplete = input.push(input.initial(), b"\xe2\x82", 1)
  assert incomplete.events.is_empty()
  let finished = input.finish(incomplete.state)
  assert finished.events == [
    input.Pressed(.Text("\u{fffd}")),
    input.Eof,
  ]
}

test test_btop_g2_input_sequence_and_paste_buffers_stay_bounded {
  let oversized_prefix = bytes.concat([b"\x1b[", repeated(59, 80)])
  let sequence = input.push(input.initial(), oversized_prefix, 10)
  assert sequence.state.pending.len() <= input.MAX_SEQUENCE_BYTES
  assert sequence.state.discarding_sequence
  assert sequence.events.is_empty()
  let sequence_end = input.push(sequence.state, b"q", 11)
  assert ! sequence_end.state.discarding_sequence
  assert sequence_end.events.is_empty()

  let paste = input.push(input.initial(), bytes.concat([b"\x1b[200~", repeated(120, input.MAX_PASTE_BYTES + 80)]), 20)
  assert paste.state.in_paste
  assert paste.state.paste_text.len() == input.MAX_PASTE_BYTES
  assert paste.state.paste_probe.len() <= 6
  assert paste.state.paste_truncated

  let ended = input.push(paste.state, b"\x1b[201~", 21)
  assert ! ended.state.in_paste
  let expected_text = repeated(120, input.MAX_PASTE_BYTES).utf8() ?? ""
  assert ended.events == [input.Paste(expected_text, true)]
}

test test_btop_g2_input_bracketed_paste_keeps_hotkeys_and_enter_inert {
  let decoded = input.push(input.initial(), b"\x1b[200~q\rs\x1b[201~", 30)

  assert decoded.events == [input.Paste("q\rs", false)]
  assert ! decoded.state.in_paste
  assert decoded.state.paste_text.is_empty()
}

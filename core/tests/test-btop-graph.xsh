use core.lib.btop_graph as graph
use core.lib.btop_terminal as terminal

const expected_braille = "⠀⠁⠂⠃⠄⠅⠆⠇⠈⠉⠊⠋⠌⠍⠎⠏⠐⠑⠒⠓⠔⠕⠖⠗⠘⠙⠚⠛⠜⠝⠞⠟⠠⠡⠢⠣⠤⠥⠦⠧⠨⠩⠪⠫⠬⠭⠮⠯⠰⠱⠲⠳⠴⠵⠶⠷⠸⠹⠺⠻⠼⠽⠾⠿⡀⡁⡂⡃⡄⡅⡆⡇⡈⡉⡊⡋⡌⡍⡎⡏⡐⡑⡒⡓⡔⡕⡖⡗⡘⡙⡚⡛⡜⡝⡞⡟⡠⡡⡢⡣⡤⡥⡦⡧⡨⡩⡪⡫⡬⡭⡮⡯⡰⡱⡲⡳⡴⡵⡶⡷⡸⡹⡺⡻⡼⡽⡾⡿⢀⢁⢂⢃⢄⢅⢆⢇⢈⢉⢊⢋⢌⢍⢎⢏⢐⢑⢒⢓⢔⢕⢖⢗⢘⢙⢚⢛⢜⢝⢞⢟⢠⢡⢢⢣⢤⢥⢦⢧⢨⢩⢪⢫⢬⢭⢮⢯⢰⢱⢲⢳⢴⢵⢶⢷⢸⢹⢺⢻⢼⢽⢾⢿⣀⣁⣂⣃⣄⣅⣆⣇⣈⣉⣊⣋⣌⣍⣎⣏⣐⣑⣒⣓⣔⣕⣖⣗⣘⣙⣚⣛⣜⣝⣞⣟⣠⣡⣢⣣⣤⣥⣦⣧⣨⣩⣪⣫⣬⣭⣮⣯⣰⣱⣲⣳⣴⣵⣶⣷⣸⣹⣺⣻⣼⣽⣾⣿"

pure utf8_inputs() -> terminal.Inputs {
  {
    term: "xterm-256color",
    colorterm: "truecolor",
    no_color: false,
    lc_all: "C.UTF-8",
    lc_ctype: "",
    lang: "",
    rows: 24,
    cols: 80,
  }
}

test test_btop_graph_exhaustively_maps_every_two_by_four_braille_dot_pattern {
  let expected = expected_braille.split("")
  assert expected.len() == 256
  for mask in range(256) {
    assert graph.braille_glyph(mask)? == expected[mask], f"incorrect braille mask {mask}"
  }
}

test test_btop_graph_pairs_time_columns_and_keeps_an_autoscale_spike {
  var history = graph.empty_history()
  history = graph.append(history, {at_ns: 1, value: 50.0, state: graph.Observed})?
  history = graph.append(history, {at_ns: 2, value: 75.0, state: graph.Observed})?
  history = graph.append(history, {at_ns: 3, value: 100.0, state: graph.Observed})?
  let frame = graph.project(history, terminal.detect(utf8_inputs()), 2, 1, false, null)?

  assert frame.symbol_set == .Braille
  assert frame.scale == 100.0
  assert frame.visible_sample_times == [1, 2, 3]
  assert frame.rows == ["\u{28a0}\u{28fe}"]
  assert history.samples.len() == 3
}

test test_btop_graph_inversion_and_gaps_preserve_sample_positions {
  var history = graph.empty_history()
  history = graph.append(history, {at_ns: 1, value: 50.0, state: graph.Observed})?
  history = graph.append(history, {at_ns: 2, value: null, state: graph.Gap})?
  history = graph.append(history, {at_ns: 3, value: 0.0, state: graph.Observed})?
  history = graph.append(history, {at_ns: 4, value: 50.0, state: graph.Observed})?
  let up = graph.project(history, terminal.detect(utf8_inputs()), 2, 1, false, 100.0)?
  let down = graph.project(history, terminal.detect(utf8_inputs()), 2, 1, true, 100.0)?

  assert up.visible_sample_times == [1, 2, 3, 4]
  assert up.visible_states == [graph.Observed, graph.Gap, graph.Observed, graph.Observed]
  assert up.rows == ["\u{2844}\u{28a0}"]
  assert down.rows == ["\u{2803}\u{2818}"]
  assert up.rows != down.rows
}

test test_btop_graph_history_retention_is_independent_of_viewport_size {
  var history = graph.empty_history()
  for index in range(graph.HISTORY_LIMIT + 1) {
    history = graph.append(
      history,
      {at_ns: index + 1, value: index.float(), state: graph.Observed},
    )?
  }

  let before = history.samples
  let narrow = graph.project(history, terminal.detect(utf8_inputs()), 1, 1, false, null)?
  let wide = graph.project(history, terminal.detect(utf8_inputs()), 20, 1, false, null)?

  assert history.samples.len() == graph.HISTORY_LIMIT
  assert history.samples[0].at_ns == 2
  assert narrow.visible_sample_times.len() == 2
  assert wide.visible_sample_times.len() == 40
  assert history.samples == before
  assert history.samples[-1].at_ns == graph.HISTORY_LIMIT + 1
}

test test_btop_graph_symbol_set_follows_captured_encoding_capabilities {
  let unicode = graph.project(
    graph.empty_history(),
    terminal.detect(utf8_inputs()),
    2,
    1,
    false,
    null,
  )?
  let ascii = graph.project(
    graph.empty_history(),
    terminal.detect({...utf8_inputs(), term: "dumb", colorterm: "", lc_all: "C"}),
    2,
    1,
    false,
    null,
  )?

  assert unicode.symbol_set == .Braille
  assert ascii.symbol_set == .Tty
  assert ascii.rows == ["  "]
}

test test_btop_graph_rejects_invalid_times_values_and_geometry {
  let zero_origin = graph.append(
    graph.empty_history(),
    {at_ns: 0, value: 0.0, state: graph.Observed},
  )?
  assert zero_origin.samples[0].at_ns == 0
  assert graph.append(
    graph.empty_history(),
    {at_ns: -1, value: 0.0, state: graph.Observed},
  ) is Err(_)
  let first = graph.append(
    graph.empty_history(),
    {at_ns: 10, value: 0.0, state: graph.Observed},
  )?
  assert graph.append(first, {at_ns: 10, value: 1.0, state: graph.Observed}) is Err(_)
  assert graph.append(first, {at_ns: 11, value: null, state: graph.Observed}) is Err(_)
  assert graph.append(first, {at_ns: 11, value: 1.0, state: graph.Gap}) is Err(_)
  assert graph.project(first, terminal.detect(utf8_inputs()), 0, 1, false, null) is Err(_)
  assert graph.project(first, terminal.detect(utf8_inputs()), 1, 0, false, null) is Err(_)
}

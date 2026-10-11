#!/bin/xsh
use core.lib.btop_loop as scheduler
use core.lib.btop_input as input
use core.lib.btop_model as model
use core.lib.btop_replay as replay
use core.lib.btop_render as renderer
use core.lib.btop_source as source
use core.lib.btop_surface as surface
use core.lib.btop_terminal as terminal
use core.lib.btop_ui as ui
use core.lib.sys_source as src
use lib.gnu

type Options = {
  replay: Str?,
  headless: Bool,
  dump_model: Str?,
  dump_screen: Str?,
  frames: Str?,
  operands: List[Str],
}

const SAMPLE_INTERVAL_NS = 1000000000
const WAIT_SLICE_MS = 100

type WaitResult = {quit: Bool, input: input.State, ui: ui.State, committed: surface.Committed}

proc draw(value: model.Model, view: ui.State, rows: Int, cols: Int, committed: surface.Committed) [io, error] -> Result[surface.Committed, Error] {
  let screen = renderer.render_view(value, view, cols, rows)
  let desired = surface.from_lines(cols, rows, screen.lines)?
  let update = surface.prepare(committed, desired)
  terminal.present(update.bytes)
  Ok(surface.complete(update, bytes.from_text(update.bytes).len()))
}

type InputResult = {quit: Bool, ui: ui.State}

pure apply_events(value: model.Model, initial_ui: ui.State, events: List[input.Event]) -> InputResult {
  var view = initial_ui
  for event in events {
    if event == input.Eof or event == input.Pressed(input.Cancel) {
      return {quit: true, ui: view}
    }
    if ! view.editing_filter and (event == input.Pressed(input.Text("q")) or event == input.Pressed(input.Text("Q"))) {
      return {quit: true, ui: view}
    }
    view = ui.reduce(view, event, value)
  }
  {quit: false, ui: view}
}

proc wait_until(
  deadline_ns: Int,
  value: model.Model,
  rows: Int,
  cols: Int,
  initial_input: input.State,
  initial_ui: ui.State,
  initial_committed: surface.Committed,
) [process, time, io, error] -> Result[WaitResult, Error] {
  var current_rows = rows
  var current_cols = cols
  var decoder = initial_input
  var view = initial_ui
  var last_drawn_ui = initial_ui
  var committed = initial_committed
  while true {
    let now = time.monotonic()?
    let now_ms = now / 1000000
    let timed = input.timeout(decoder, now_ms)
    decoder = timed.state
    let timed_input = apply_events(value, view, timed.events)
    view = timed_input.ui
    if timed_input.quit {
      return Ok({quit: true, input: decoder, ui: view, committed: committed})
    }
    if view != last_drawn_ui {
      committed = draw(value, view, current_rows, current_cols, committed)?
      last_drawn_ui = view
    }
    if now >= deadline_ns {
      return Ok({quit: false, input: decoder, ui: view, committed: committed})
    }
    let remaining_ms = (deadline_ns - now) / 1000000
    var wait_ms = if remaining_ms <= 0 { 1 } else if remaining_ms > WAIT_SLICE_MS { WAIT_SLICE_MS } else { remaining_ms }
    if decoder.escape_deadline_ms != null {
      let until_escape = (decoder.escape_deadline_ms ?? now_ms) - now_ms
      if until_escape <= 0 {
        wait_ms = 1
      } else if until_escape < wait_ms {
        wait_ms = until_escape
      }
    }
    let events = unix.poll_fd(0, ["readable"], timeout_ms: wait_ms)?
    if "readable" in events {
      let chunk = unix.read_fd(0, 256)?
      let read_now_ms = time.monotonic()? / 1000000
      let decoded = if chunk.is_empty() { input.finish(decoder) } else { input.push(decoder, chunk, read_now_ms) }
      decoder = decoded.state
      let received = apply_events(value, view, decoded.events)
      view = received.ui
      if received.quit {
        return Ok({quit: true, input: decoder, ui: view, committed: committed})
      }
      if view != last_drawn_ui {
        committed = draw(value, view, current_rows, current_cols, committed)?
        last_drawn_ui = view
      }
    }
    let size = unix.window_size(1)?
    if size.rows != current_rows or size.cols != current_cols {
      current_rows = size.rows
      current_cols = size.cols
      view = ui.resize(view, value, renderer.process_capacity(current_rows))
      committed = draw(value, view, current_rows, current_cols, committed)?
    }
  }
  Ok({quit: false, input: decoder, ui: view, committed: committed})
}

pure limited_observations(observations: List[model.Observation], count: Int?) -> List[model.Observation] {
  if count == null or (count ?? 0) >= observations.len() {
    return observations
  }
  observations |> take(count ?? 0)
}

pure limited_models(frames: List[model.Model], count: Int?) -> List[model.Model] {
  if count == null or (count ?? 0) >= frames.len() {
    return frames
  }
  frames |> take(count ?? 0)
}

proc parse_frame_limit(value: Str?) -> Result[Int?, Error] {
  if value == null {
    return Ok(null)
  }
  let parsed = src.parse_integer(value)
  if parsed == null or (parsed ?? 0) <= 0 {
    return Err(error.failure("--frames requires a positive integer"))
  }
  Ok(parsed)
}

proc dump_frames(frames: List[model.Model], model_path: Str?, screen_path: Str?, rows: Int, cols: Int) [fs, error] -> Result[Unit, Error] {
  if let output_path = model_path {
    var lines: List[Str] = []
    for frame in frames {
      lines += [json.encode(frame, pretty: false)?]
    }
    fp"{output_path}".write(bytes.from_text(lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })))?
  }
  if let output_path = screen_path {
    var lines: List[Str] = []
    for frame in frames {
      let screen = renderer.render(frame, cols, rows)
      lines += [json.encode({at_ns: frame.at_ns, width: screen.width, height: screen.height, too_small: screen.too_small, lines: screen.lines}, pretty: false)?]
    }
    fp"{output_path}".write(bytes.from_text(lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })))?
  }
  return
}

proc headless_replay(bundle: replay.Bundle, count: Int?, dump_model_path: Str?, dump_screen_path: Str?) [fs, error] -> Result[Unit, Error] {
  let observations = limited_observations(bundle.observations, count)
  let frames = limited_models(replay.models(observations), count)
  dump_frames(frames, dump_model_path, dump_screen_path, bundle.display.rows, bundle.display.cols)
}

proc interactive_replay(bundle: replay.Bundle, count: Int?) [process, time, io, error] -> Result[Unit, Error] {
  let selected_observations = limited_observations(bundle.observations, count)
  let frames = replay.models(selected_observations)
  let session = terminal.begin_with(bundle.display)?
  defer { terminal.finish(session)? }
  if frames.is_empty() {
    return
  }

  let started_at = time.monotonic()?
  let first_at = selected_observations[0].at_ns
  var rows = session.capabilities.rows
  var cols = session.capabilities.cols
  var decoder = input.initial()
  var view = ui.initial(frames[0], renderer.process_capacity(rows))
  var committed = surface.initial()
  committed = draw(frames[0], view, rows, cols, committed)?
  for index in range(1, frames.len()) {
    let deadline = started_at + selected_observations[index].at_ns - first_at
    let waited = wait_until(deadline, frames[index - 1], rows, cols, decoder, view, committed)?
    decoder = waited.input
    view = waited.ui
    committed = waited.committed
    if waited.quit {
      return
    }
    let size = unix.window_size(1)?
    rows = size.rows
    cols = size.cols
    view = ui.refresh(view, frames[index])
    view = ui.resize(view, frames[index], renderer.process_capacity(rows))
    committed = draw(frames[index], view, rows, cols, committed)?
  }
  return
}

proc interactive_live(count: Int?) [fs, process, time, io, error] -> Result[Unit, Error] {
  let session = terminal.begin()?
  defer { terminal.finish(session)? }

  let root = fs.open_root(/)?
  defer root.close()
  let units = system.execution_units()?
  var previous: model.Observation? = null
  var sampled = 0
  let first_started_ns = time.monotonic()?
  let first = scheduler.first(first_started_ns, SAMPLE_INTERVAL_NS)?
  var schedule = first.state
  var observation = source.capture(
    root,
    "live:/",
    first_started_ns,
    units.page_size_bytes,
    units.clock_ticks_per_second,
  )?
  let _ = scheduler.complete(first.ticket, time.monotonic()?)?
  var rows = session.capabilities.rows
  var cols = session.capabilities.cols
  var decoder = input.initial()
  var current = model.step(observation, previous)
  var view = ui.initial(current, renderer.process_capacity(rows))
  var committed = surface.initial()
  while true {
    committed = draw(current, view, rows, cols, committed)?
    sampled += 1
    if count != null and sampled >= (count ?? 0) {
      break
    }
    let waited = wait_until(schedule.next_due_ns, current, rows, cols, decoder, view, committed)?
    decoder = waited.input
    view = waited.ui
    committed = waited.committed
    if waited.quit {
      break
    }
    let due = scheduler.take_due(schedule, time.monotonic()?)?
    schedule = due.state
    if let ticket = due.ticket {
      let next_observation = source.capture(
        root,
        "live:/",
        ticket.started_ns,
        units.page_size_bytes,
        units.clock_ticks_per_second,
      )?
      let _ = scheduler.complete(ticket, time.monotonic()?)?
      previous = observation
      observation = next_observation
      current = model.step(observation, previous)
      view = ui.refresh(view, current)
    } else {
      return Err(error.failure("sample deadline expired without a due ticket"))
    }
    let size = unix.window_size(1)?
    rows = size.rows
    cols = size.cols
    view = ui.resize(view, current, renderer.process_capacity(rows))
  }
  return
}

proc main(...argv: List[Str]) [fs, process, env, time, io, error] {
  let opts: Options = cli.applet(
    argv,
    {
      gnu: {status: 2},
      replay: {form: "--replay BUNDLE"},
      headless: {form: "--headless", default: false},
      dump_model: {form: "--dump-model FILE"},
      dump_screen: {form: "--dump-screen FILE"},
      frames: {form: "--frames N"},
      operands: {form: "...ARG"},
    },
  )?
  if ! opts.operands.is_empty() {
    gnu.error("btop: unexpected operand")
    exit 2
  }
  let count = match parse_frame_limit(opts.frames) {
    Ok(value) => value
    Err(failure) => {
      gnu.error(f"btop: {failure.message}")
      exit 2
    }
  }
  if opts.headless and opts.replay == null {
    gnu.error("btop: --headless requires --replay")
    exit 2
  }
  if (opts.dump_model != null or opts.dump_screen != null) and ! opts.headless {
    gnu.error("btop: dumps require --headless")
    exit 2
  }

  if let bundle = opts.replay {
    let loaded = replay.load(Path(bundle))?
    if opts.headless {
      headless_replay(loaded, count, opts.dump_model, opts.dump_screen)?
    } else {
      interactive_replay(loaded, count)?
    }
  } else {
    interactive_live(count)?
  }
}

#!/bin/xsh
use core.lib.btop_loop as scheduler
use core.lib.btop_input as input
use core.lib.btop_graphs as graph_series
use core.lib.btop_model as model
use core.lib.btop_replay as replay
use core.lib.btop_render as renderer
use core.lib.btop_schedule as collection_schedule
use core.lib.btop_source as source
use core.lib.btop_surface as surface
use core.lib.btop_terminal as terminal
use core.lib.btop_ui as ui
use core.lib.sys_proc as sysproc
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

const WAIT_SLICE_MS = 100

type WaitResult = {
  quit: Bool,
  input: input.State,
  ui: ui.State,
  committed: surface.Committed,
  graphs: graph_series.Histories,
}

proc draw(
  value: model.Model,
  view: ui.State,
  rows: Int,
  cols: Int,
  committed: surface.Committed,
  histories: graph_series.Histories,
  capabilities: terminal.Capabilities,
  local_display: List[sysproc.LocalProcessDisplay] = [],
) [io, error] -> Result[surface.Committed, Error] {
  let screen = renderer.render_view_with_graphs(value, view, cols, rows, local_display, histories, capabilities)?
  let desired = surface.from_lines_with_roles(cols, rows, screen.lines, screen.row_roles)?
  let update = surface.prepare(committed, desired, capabilities.color_depth)
  terminal.present(update.bytes)
  Ok(surface.complete(update, bytes.from_text(update.bytes).len()))
}

type InputResult = {quit: Bool, ui: ui.State}

pure apply_events(
  value: model.Model,
  initial_ui: ui.State,
  events: List[input.Event],
  local_display: List[sysproc.LocalProcessDisplay] = [],
) -> InputResult {
  var view = initial_ui
  for event in events {
    if event == .Eof or event == input.Pressed(input.Cancel) {
      return {quit: true, ui: view}
    }
    if ! view.editing_filter and (event == input.Pressed(.Text("q")) or event == input.Pressed(.Text("Q"))) {
      return {quit: true, ui: view}
    }
    view = ui.reduce(view, event, value, local_display)
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
  histories: graph_series.Histories,
  capabilities: terminal.Capabilities,
  local_display: List[sysproc.LocalProcessDisplay] = [],
) [process, time, io, error] -> Result[WaitResult, Error] {
  var current_rows = rows
  var current_cols = cols
  var decoder = initial_input
  var view = initial_ui
  var last_drawn_ui = initial_ui
  var committed = initial_committed
  var current_histories = histories
  while true {
    let now = time.monotonic()?
    let now_ms = now / 1000000
    let timed = input.timeout(decoder, now_ms)
    decoder = timed.state
    let timed_input = apply_events(value, view, timed.events, local_display)
    view = timed_input.ui
    if timed_input.quit {
      return Ok({quit: true, input: decoder, ui: view, committed: committed, graphs: current_histories})
    }
    if view != last_drawn_ui {
      if view.network_interface != last_drawn_ui.network_interface {
        current_histories = graph_series.observe(current_histories, value, view.network_interface)?
      }
      committed = draw(value, view, current_rows, current_cols, committed, current_histories, capabilities, local_display)?
      last_drawn_ui = view
    }
    if now >= deadline_ns {
      return Ok({quit: false, input: decoder, ui: view, committed: committed, graphs: current_histories})
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
      let received = apply_events(value, view, decoded.events, local_display)
      view = received.ui
      if received.quit {
        return Ok({quit: true, input: decoder, ui: view, committed: committed, graphs: current_histories})
      }
      if view != last_drawn_ui {
        if view.network_interface != last_drawn_ui.network_interface {
          current_histories = graph_series.observe(current_histories, value, view.network_interface)?
        }
        committed = draw(value, view, current_rows, current_cols, committed, current_histories, capabilities, local_display)?
        last_drawn_ui = view
      }
    }
    let size = unix.window_size(1)?
    if size.rows != current_rows or size.cols != current_cols {
      current_rows = size.rows
      current_cols = size.cols
      view = ui.resize(view, value, renderer.process_capacity(current_rows))
      committed = draw(value, view, current_rows, current_cols, committed, current_histories, capabilities, local_display)?
    }
  }
  Ok({quit: false, input: decoder, ui: view, committed: committed, graphs: current_histories})
}

pure limited_observations(observations: List[model.Observation], count: Int?) -> List[model.Observation] {
  return observations when count == null or (count) >= observations.len()
  observations |> take(count)
}

pure limited_models(frames: List[model.Model], count: Int?) -> List[model.Model] {
  return frames when count == null or (count) >= frames.len()
  frames |> take(count)
}

proc parse_frame_limit(value: Str?) -> Result[Int?, Error] {
  return Ok(null) when value == null
  let parsed = src.parse_integer(value)
  if parsed == null or (parsed) <= 0 {
    return Err(error.failure("--frames requires a positive integer"))
  }
  Ok(parsed)
}

proc dump_frames(
  frames: List[model.Model],
  model_path: Str?,
  screen_path: Str?,
  display: terminal.Inputs,
) [fs, error] {
  if let output_path = model_path {
    let lines = collect {
      for frame in frames {
        yield model.encode_wire_json(frame)?
      }
    }
    fp"{output_path}".write(bytes.from_text(lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })))
  }
  if let output_path = screen_path {
    var histories = graph_series.empty()
    let capabilities = terminal.detect(display)
    var lines = []
    for frame in frames {
      let view = ui.initial(frame, renderer.process_capacity(display.rows))
      histories = graph_series.observe(histories, frame, view.network_interface)?
      let screen = renderer.render_view_with_graphs(
        frame,
        view,
        display.cols,
        display.rows,
        [],
        histories,
        capabilities,
      )?
      lines += [json.encode({at_ns: frame.at_ns, width: screen.width, height: screen.height, too_small: screen.too_small, lines: screen.lines}, pretty: false)?]
    }
    fp"{output_path}".write(bytes.from_text(lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })))
  }
  return
}

proc headless_replay(bundle: replay.Bundle, count: Int?, dump_model_path: Str?, dump_screen_path: Str?) [fs, error] {
  let observations = limited_observations(bundle.observations, count)
  let frames = limited_models(replay.models(observations), count)
  dump_frames(frames, dump_model_path, dump_screen_path, bundle.display)
}

proc interactive_replay(bundle: replay.Bundle, count: Int?) [process, time, io, error] {
  let selected_observations = limited_observations(bundle.observations, count)
  let frames = replay.models(selected_observations)
  let session = terminal.begin_with(bundle.display)?
  defer { terminal.finish(session) }
  return when frames.is_empty()

  let started_at = time.monotonic()?
  let first_at = selected_observations[0].at_ns
  var rows = session.capabilities.rows
  var cols = session.capabilities.cols
  var histories = graph_series.empty()
  var decoder = input.initial()
  var view = ui.initial(frames[0], renderer.process_capacity(rows))
  histories = graph_series.observe(histories, frames[0], view.network_interface)?
  var committed = surface.initial()
  committed = draw(frames[0], view, rows, cols, committed, histories, session.capabilities)?
  for index in range(1, frames.len()) {
    let deadline = started_at + selected_observations[index].at_ns - first_at
    let waited = wait_until(
      deadline,
      frames[index - 1],
      rows,
      cols,
      decoder,
      view,
      committed,
      histories,
      session.capabilities,
    )?
    decoder = waited.input
    view = waited.ui
    committed = waited.committed
    return when waited.quit
    let size = unix.window_size(1)?
    rows = size.rows
    cols = size.cols
    view = ui.refresh(view, frames[index])
    view = ui.resize(view, frames[index], renderer.process_capacity(rows))
    histories = graph_series.observe(histories, frames[index], view.network_interface)?
    committed = draw(frames[index], view, rows, cols, committed, histories, session.capabilities)?
  }
  return
}

proc interactive_live(count: Int?) [fs, process, time, io, error] {
  let session = terminal.begin()?
  defer { terminal.finish(session) }

  let root = fs.open_root(/)?
  defer root.close()
  let units = system.execution_units()?
  var previous: model.Observation? = null
  var sampled = 0
  let first_started_ns = time.monotonic()?
  let first = scheduler.first(first_started_ns, scheduler.SAMPLE_INTERVAL_NS)?
  var schedule = first.state
  let first_collection = collection_schedule.take_due(
    collection_schedule.initial(first_started_ns)?,
    first_started_ns,
  )?
  var collection_state = first_collection.state
  var counters_due = false
  var gauges_due = false
  var storage_due = false
  for item in first_collection.due {
    if item.class == .Counters {
      counters_due = true
    } else if item.class == .Gauges {
      gauges_due = true
    } else if item.class == .Inventory or item.class == .Blocking {
      storage_due = true
    }
  }
  if ! counters_due or ! gauges_due or ! storage_due {
    return Err(error.failure("first monitor observation is missing due source classes"))
  }
  var observation = source.capture(
    root,
    "live:/",
    first_started_ns,
    units.page_size_bytes,
    units.clock_ticks_per_second,
  )?
  var cached_mounts = observation.storage.mounts
  var cached_mounts_at_ns = observation.storage.mounts_observed_at_ns
  let _ = scheduler.complete(first.ticket, time.monotonic()?)?
  var rows = session.capabilities.rows
  var cols = session.capabilities.cols
  var decoder = input.initial()
  let first_model = model.step_cached(observation, previous, model.empty_metadata_cache("live:/"))
  var metadata_cache = first_model.metadata
  var current = first_model.value
  var local_display = sysproc.collect_local_display(root, observation.processes.processes)
  var view = ui.initial(current, renderer.process_capacity(rows))
  var histories = graph_series.observe(graph_series.empty(), current, view.network_interface)?
  var committed = surface.initial()
  while true {
    committed = draw(current, view, rows, cols, committed, histories, session.capabilities, local_display)?
    sampled += 1
    break when count != null and sampled >= (count)
    let waited = wait_until(
      schedule.next_due_ns,
      current,
      rows,
      cols,
      decoder,
      view,
      committed,
      histories,
      session.capabilities,
      local_display,
    )?
    decoder = waited.input
    view = waited.ui
    committed = waited.committed
    break when waited.quit
    let due = scheduler.take_due(schedule, time.monotonic()?)?
    schedule = due.state
    if let ticket = due.ticket {
      let collection_plan = collection_schedule.take_due(collection_state, ticket.started_ns)?
      collection_state = collection_plan.state
      var counters_due = false
      var gauges_due = false
      var storage_due = false
      for item in collection_plan.due {
        if item.class == .Counters {
          counters_due = true
        } else if item.class == .Gauges {
          gauges_due = true
        } else if item.class == .Inventory or item.class == .Blocking {
          storage_due = true
        }
      }
      if ! counters_due {
        return Err(error.failure("sample deadline expired without due counter sources"))
      }
      let next_observation = source.capture(
        root,
        "live:/",
        ticket.started_ns,
        units.page_size_bytes,
        units.clock_ticks_per_second,
        if gauges_due { null } else { observation.telemetry },
        cached_mounts: if storage_due { null } else { cached_mounts },
        cached_mounts_at_ns: if storage_due { null } else { cached_mounts_at_ns },
      )?
      let _ = scheduler.complete(ticket, time.monotonic()?)?
      previous = observation
      observation = next_observation
      cached_mounts = observation.storage.mounts
      cached_mounts_at_ns = observation.storage.mounts_observed_at_ns
      local_display = sysproc.collect_local_display(root, observation.processes.processes)
      let next_model = model.step_cached(observation, previous, metadata_cache, gap: ticket.coalesced > 0)
      metadata_cache = next_model.metadata
      current = next_model.value
      view = ui.refresh(view, current, local_display)
      histories = graph_series.observe(histories, current, view.network_interface)?
    } else {
      return Err(error.failure("sample deadline expired without a due ticket"))
    }
    let size = unix.window_size(1)?
    rows = size.rows
    cols = size.cols
    view = ui.resize(view, current, renderer.process_capacity(rows), local_display)
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
    let loaded = replay.load(fp"{bundle}")?
    if opts.headless {
      headless_replay(loaded, count, opts.dump_model, opts.dump_screen)
    } else {
      interactive_replay(loaded, count)
    }
  } else {
    interactive_live(count)
  }
}

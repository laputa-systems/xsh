#!/bin/xsh
use core.lib.btop_model as model
use core.lib.btop_replay as replay
use core.lib.btop_render as renderer
use core.lib.btop_source as source
use core.lib.sys_source as src
use lib.gnu
use tui

type Options = {
  replay: Str?,
  headless: Bool,
  dump_model: Str?,
  dump_screen: Str?,
  frames: Str?,
  operands: List[Str],
}

type TerminalSession = {attrs: UnixTtyAttrs, rows: Int, cols: Int}

const SAMPLE_INTERVAL_NS = 1000000000
const WAIT_SLICE_MS = 100

proc terminal_session() [process, error] -> Result[TerminalSession, Error] {
  if ! unix.isatty(0) or ! unix.isatty(1) {
    return Err(error.failure("btop requires interactive stdin and stdout terminals"))
  }
  let size = unix.window_size(1)?
  if size.rows <= 0 or size.cols <= 0 {
    return Err(error.failure("btop could not read a usable terminal size"))
  }
  Ok({attrs: unix.tty_attrs(0)?, rows: size.rows, cols: size.cols})
}

proc draw(value: model.Model, rows: Int, cols: Int) [io] {
  let screen = renderer.render(value, cols, rows)
  gnu.write_text(tui.clear() + tui.home() + screen.lines.join("\n"))
}

pure requested_quit(input: Bytes) -> Bool {
  return true when input.is_empty()
  for byte in input {
    if byte == 113 or byte == 81 or byte == 3 {
      return true
    }
  }
  false
}

proc wait_until(deadline_ns: Int, value: model.Model, rows: Int, cols: Int) [process, time, io, error] -> Result[Bool, Error] {
  var current_rows = rows
  var current_cols = cols
  while true {
    let now = time.monotonic()?
    if now >= deadline_ns {
      return Ok(false)
    }
    let remaining_ms = (deadline_ns - now) / 1000000
    let wait_ms = if remaining_ms <= 0 { 1 } else if remaining_ms > WAIT_SLICE_MS { WAIT_SLICE_MS } else { remaining_ms }
    let events = unix.poll_fd(0, ["readable"], timeout_ms: wait_ms)?
    if "readable" in events {
      if requested_quit(unix.read_fd(0, 256)?) {
        return Ok(true)
      }
    }
    let size = unix.window_size(1)?
    if size.rows != current_rows or size.cols != current_cols {
      current_rows = size.rows
      current_cols = size.cols
      draw(value, current_rows, current_cols)
    }
  }
  Ok(false)
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

proc dump_frames(frames: List[model.Model], model_path: Str?, screen_path: Str?) [fs, error] -> Result[Unit, Error] {
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
      let screen = renderer.render(frame, 80, 24)
      lines += [json.encode({at_ns: frame.at_ns, width: screen.width, height: screen.height, too_small: screen.too_small, lines: screen.lines}, pretty: false)?]
    }
    fp"{output_path}".write(bytes.from_text(lines.join("\n") + (if lines.is_empty() { "" } else { "\n" })))?
  }
  return
}

proc headless_replay(bundle_path: Str, count: Int?, dump_model_path: Str?, dump_screen_path: Str?) [fs, error] -> Result[Unit, Error] {
  let observations = replay.load(Path(bundle_path))?
  let frames = limited_models(replay.models(limited_observations(observations, count)), count)
  dump_frames(frames, dump_model_path, dump_screen_path)
}

proc interactive_replay(observations: List[model.Observation], count: Int?) [process, time, io, error] -> Result[Unit, Error] {
  let selected_observations = limited_observations(observations, count)
  let frames = replay.models(selected_observations)
  let terminal = terminal_session()?
  defer { unix.set_tty_attrs(terminal.attrs, fd: 0)? }
  defer { gnu.write_text(tui.reset() + tui.show_cursor()) }
  let raw = unix.tty_mode(terminal.attrs, "raw")?
  unix.set_tty_attrs(raw, fd: 0)?
  gnu.write_text(tui.clear() + tui.hide_cursor())
  if frames.is_empty() {
    return
  }

  let started_at = time.monotonic()?
  let first_at = selected_observations[0].at_ns
  var rows = terminal.rows
  var cols = terminal.cols
  draw(frames[0], rows, cols)
  for index in range(1, frames.len()) {
    let deadline = started_at + selected_observations[index].at_ns - first_at
    if wait_until(deadline, frames[index - 1], rows, cols)? {
      return
    }
    let size = unix.window_size(1)?
    rows = size.rows
    cols = size.cols
    draw(frames[index], rows, cols)
  }
  return
}

proc interactive_live(count: Int?) [fs, process, time, io, error] -> Result[Unit, Error] {
  let terminal = terminal_session()?
  defer { unix.set_tty_attrs(terminal.attrs, fd: 0)? }
  defer { gnu.write_text(tui.reset() + tui.show_cursor()) }
  let raw = unix.tty_mode(terminal.attrs, "raw")?
  unix.set_tty_attrs(raw, fd: 0)?
  gnu.write_text(tui.clear() + tui.hide_cursor())

  let root = fs.open_root(/)?
  defer root.close()
  let units = system.execution_units()?
  var previous: model.Observation? = null
  var sampled = 0
  var rows = terminal.rows
  var cols = terminal.cols
  while true {
    let observation = source.capture(
      root,
      "live:/",
      time.monotonic()?,
      units.page_size_bytes,
      units.clock_ticks_per_second,
    )?
    let current = model.step(observation, previous)
    draw(current, rows, cols)
    previous = observation
    sampled += 1
    if count != null and sampled >= (count ?? 0) {
      break
    }
    if wait_until(observation.at_ns + SAMPLE_INTERVAL_NS, current, rows, cols)? {
      break
    }
    let size = unix.window_size(1)?
    rows = size.rows
    cols = size.cols
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
    if opts.headless {
      headless_replay(bundle, count, opts.dump_model, opts.dump_screen)?
    } else {
      interactive_replay(replay.load(Path(bundle))?, count)?
    }
  } else {
    interactive_live(count)?
  }
}

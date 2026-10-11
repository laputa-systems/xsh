##! Loads a finite btop replay bundle without consulting the live host.
##!
##! Frame directories are derived from their manifest order, while timestamps,
##! page size, tick rate, and source identity come from the bundle manifest.
use btop_model as model
use btop_loop as scheduler
use btop_source as source
use btop_terminal as terminal
use sys_mount as mounts
use sys_source as src
use system_report as report

type ReplayTimestamp = {at_ns: Int}
type ReplayMountCapacity = {
  frame_index: Int,
  at_ns: Int,
  sequence: Int,
  mount_id: Int,
  major: Int,
  minor: Int,
  target: Str,
  usage_state: report.ObservationState,
  usage_total_bytes: Str?,
  usage_used_bytes: Str?,
  usage_available_bytes: Str?,
}
type ReplayManifest = {
  schema: Int,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
  source_id: Str,
  display: terminal.Inputs,
  frames: List[ReplayTimestamp],
}

## Holds the raw observations and recorded display policy for one replay.
export type Bundle = {observations: List[model.Observation], display: terminal.Inputs}

proc capture_frame(
  root: FsRoot,
  index: Int,
  source_id: Str,
  at_ns: Int,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
  captured_usage: List[mounts.CapturedUsage],
) [fs, error] -> Result[model.Observation, Error] {
  let frame_root = root.open_root(fp"frames/{index}")?
  defer frame_root.close()
  source.capture(
    frame_root,
    source_id,
    at_ns,
    page_size_bytes,
    clock_ticks_per_second,
    captured_mount_usage: captured_usage,
  )
}

pure captured_capacity_bytes(value: Str?) -> Result[Int?, Error] {
  if value == null {
    return Ok(null)
  }
  let text = value ?? ""
  let parsed = src.parse_integer(text)
  let parsed_value = parsed ?? -1
  if parsed == null or parsed_value < 0 or f"{parsed_value}" != text {
    return Err(error.failure("captured mount capacity bytes must be canonical nonnegative decimal strings"))
  }
  Ok(parsed_value)
}

proc read_captured_usage(root: FsRoot, frames: List[ReplayTimestamp]) [fs, error] -> Result[Map[List[mounts.CapturedUsage]], Error] {
  let file = src.read_source_text(root, p"observations.jsonl", max_bytes: 16777216)
  if file.observation.state != report.Observed or file.observation.value == null {
    return Err(error.failure("btop replay requires a complete observations.jsonl file"))
  }
  let observations_text = file.observation.value ?? ""
  let lines = if observations_text == "" { [] } else { observations_text.lines() }
  if lines.len() > 65536 {
    return Err(error.failure("btop replay captured observations exceed their row limit"))
  }

  var by_frame: Map[List[mounts.CapturedUsage]] = {}
  var last_frame_index = -1
  var expected_sequence = 0
  for line in lines {
    if line.byte_len() > 16384 {
      return Err(error.failure("btop replay captured observation exceeds its row size limit"))
    }
    guard let decoded = json.decode(line) else { |failure|
      return Err(error.failure(f"invalid btop replay captured observation: {failure.message}"))
    }
    guard let row = decoded.require(ReplayMountCapacity) else { |failure|
      return Err(error.failure(f"invalid btop replay captured observation fields: {failure.message}"))
    }
    if row.frame_index < 0 or row.frame_index >= frames.len() or row.at_ns != frames[row.frame_index].at_ns or
        row.frame_index < last_frame_index {
      return Err(error.failure("btop replay captured observations are out of frame order"))
    }
    if row.frame_index != last_frame_index {
      last_frame_index = row.frame_index
      expected_sequence = 0
    }
    if row.sequence != expected_sequence {
      return Err(error.failure("btop replay captured observation sequence is not contiguous"))
    }
    expected_sequence += 1
    let key = f"{row.frame_index}"
    let existing = if let Ok(values) = by_frame.get(key) { values } else { [] }
    let total_bytes = captured_capacity_bytes(row.usage_total_bytes)?
    let used_bytes = captured_capacity_bytes(row.usage_used_bytes)?
    let available_bytes = captured_capacity_bytes(row.usage_available_bytes)?
    by_frame = by_frame.set(key, existing + [
      {
        mount_id: row.mount_id,
        major: row.major,
        minor: row.minor,
        target: row.target,
        usage_state: row.usage_state,
        usage_total_bytes: total_bytes,
        usage_used_bytes: used_bytes,
        usage_available_bytes: available_bytes,
      },
    ])
  }
  Ok(by_frame)
}

## Captures every frame in order and rejects ambiguous timing or metadata.
export proc load(bundle_path: Path) [fs, error] -> Result[Bundle, Error] {
  let root = fs.open_root(bundle_path)?
  defer root.close()
  let manifest_source = src.read_source_text(root, p"manifest.json", max_bytes: 65536)
  if manifest_source.observation.state != report.Observed or manifest_source.observation.value == null {
    return Err(error.failure("btop replay requires a complete bounded manifest.json file"))
  }
  let manifest_text = manifest_source.observation.value ?? ""
  guard let decoded = json.decode(manifest_text) else { |failure|
    return Err(error.failure(f"invalid btop replay manifest: {failure.message}"))
  }
  guard let manifest = decoded.require(ReplayManifest) else { |failure|
    return Err(error.failure(f"invalid btop replay manifest fields: {failure.message}"))
  }
  if manifest.schema != 3 {
    return Err(error.failure(f"unsupported btop replay schema {manifest.schema}"))
  }
  if manifest.page_size_bytes <= 0 or manifest.clock_ticks_per_second <= 0 or manifest.source_id == "" {
    return Err(error.failure("btop replay metadata requires positive rates and a source identity"))
  }
  if manifest.display.rows <= 0 or manifest.display.cols <= 0 or
      manifest.display.rows > 2000 or manifest.display.cols > 2000 or
      manifest.display.term.byte_len() > 256 or manifest.display.colorterm.byte_len() > 256 or
      manifest.display.lc_all.byte_len() > 256 or manifest.display.lc_ctype.byte_len() > 256 or
      manifest.display.lang.byte_len() > 256 {
    return Err(error.failure("btop replay display inputs exceed their bounds"))
  }
  if manifest.frames.is_empty() or manifest.frames.len() > 10000 {
    return Err(error.failure("btop replay frame count must be between 1 and 10000"))
  }

  let captured_usage = read_captured_usage(root, manifest.frames)?

  var observations: List[model.Observation] = []
  var previous_at_ns: Int? = null
  for index in range(manifest.frames.len()) {
    let at_ns = manifest.frames[index].at_ns
    if at_ns < 0 or (previous_at_ns != null and at_ns <= (previous_at_ns ?? -1)) {
      return Err(error.failure("btop replay timestamps must be nonnegative and strictly increasing"))
    }
    let frame_usage = if let Ok(values) = captured_usage.get(f"{index}") { values } else { [] }
    observations += [capture_frame(
      root,
      index,
      manifest.source_id,
      at_ns,
      manifest.page_size_bytes,
      manifest.clock_ticks_per_second,
      frame_usage,
    )?]
    previous_at_ns = at_ns
  }
  Ok({observations: observations, display: manifest.display})
}

## Applies the same interval reducer to every frame without waiting for wall time.
export pure models(observations: List[model.Observation]) -> List[model.Model] {
  var frames: List[model.Model] = []
  var previous: model.Observation? = null
  var metadata = model.empty_metadata_cache(if observations.is_empty() { "" } else { observations[0].source_id })
  for current in observations {
    let before = previous ?? current
    let has_previous = previous != null
    let elapsed_ns = if has_previous and before.at_ns >= 0 and current.at_ns > before.at_ns {
      current.at_ns - before.at_ns
    } else {
      0
    }
    let gap = elapsed_ns >= scheduler.SAMPLE_INTERVAL_NS * 2
    let next = model.step_cached(current, previous, metadata, gap: gap)
    frames += [next.value]
    metadata = next.metadata
    previous = current
  }
  frames
}

##! Loads a finite btop replay bundle without consulting the live host.
##!
##! Frame directories are derived from their manifest order, while timestamps,
##! page size, tick rate, and source identity come from the bundle manifest.
use btop_model as model
use btop_loop as scheduler
use btop_source as source
use btop_terminal as terminal

type ReplayTimestamp = {at_ns: Int}
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
) [fs, error] -> Result[model.Observation, Error] {
  let frame_root = root.open_root(fp"frames/{index}")?
  defer frame_root.close()
  source.capture(frame_root, source_id, at_ns, page_size_bytes, clock_ticks_per_second)
}

## Captures every frame in order and rejects ambiguous timing or metadata.
export proc load(bundle_path: Path) [fs, error] -> Result[Bundle, Error] {
  let root = fs.open_root(bundle_path)?
  defer root.close()
  let manifest_text = root.read_text(p"manifest.json")?
  guard let decoded = json.decode(manifest_text) else { |failure|
    return Err(error.failure(f"invalid btop replay manifest: {failure.message}"))
  }
  guard let manifest = decoded.require(ReplayManifest) else { |failure|
    return Err(error.failure(f"invalid btop replay manifest fields: {failure.message}"))
  }
  if manifest.schema != 2 {
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
  if manifest.frames.is_empty() {
    return Err(error.failure("btop replay must contain at least one frame"))
  }

  var observations: List[model.Observation] = []
  var previous_at_ns: Int? = null
  for index in range(manifest.frames.len()) {
    let at_ns = manifest.frames[index].at_ns
    if at_ns < 0 or (previous_at_ns != null and at_ns <= (previous_at_ns ?? -1)) {
      return Err(error.failure("btop replay timestamps must be nonnegative and strictly increasing"))
    }
    observations += [capture_frame(
      root,
      index,
      manifest.source_id,
      at_ns,
      manifest.page_size_bytes,
      manifest.clock_ticks_per_second,
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

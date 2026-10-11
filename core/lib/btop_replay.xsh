##! Loads a finite btop replay bundle without consulting the live host.
##!
##! Frame directories are derived from their manifest order, while timestamps,
##! page size, tick rate, and source identity come from the bundle manifest.
use btop_model as model
use btop_source as source

type ReplayTimestamp = {at_ns: Int}
type ReplayManifest = {
  schema: Int,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
  source_id: Str,
  frames: List[ReplayTimestamp],
}

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
export proc load(bundle_path: Path) [fs, error] -> Result[List[model.Observation], Error] {
  let root = fs.open_root(bundle_path)?
  defer root.close()
  let manifest_text = root.read_text(p"manifest.json")?
  guard let decoded = json.decode(manifest_text) else { |failure|
    return Err(error.failure(f"invalid btop replay manifest: {failure.message}"))
  }
  guard let manifest = decoded.require(ReplayManifest) else { |failure|
    return Err(error.failure(f"invalid btop replay manifest fields: {failure.message}"))
  }
  if manifest.schema != 1 {
    return Err(error.failure(f"unsupported btop replay schema {manifest.schema}"))
  }
  if manifest.page_size_bytes <= 0 or manifest.clock_ticks_per_second <= 0 or manifest.source_id == "" {
    return Err(error.failure("btop replay metadata requires positive rates and a source identity"))
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
  Ok(observations)
}

## Applies the same interval reducer to every frame without waiting for wall time.
export pure models(observations: List[model.Observation]) -> List[model.Model] {
  var frames: List[model.Model] = []
  var previous: model.Observation? = null
  for current in observations {
    frames += [model.step(current, previous)]
    previous = current
  }
  frames
}

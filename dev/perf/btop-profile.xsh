use core.lib.btop_model as model
use core.lib.btop_render as renderer
use core.lib.btop_source as source
use core.lib.btop_surface as surface
use core.lib.sys_proc as sysproc
use core.tests.support.btop as fixture
use core.tests.support.btop_process_fixture as process_fixture

proc measured_process_join(current: List[sysproc.Process], previous: List[sysproc.Process]) [time, error] -> Int {
  let started_ns = time.monotonic()?
  let joined = model.process_join(current, previous)
  let _ = joined.len()
  time.monotonic()? - started_ns
}

pure median(values: List[Int]) -> Int {
  let ordered = values |> sort
  ordered[ordered.len() / 2]
}

proc main() [fs, time, io, error] {
  let root = fs.tempdir()?
  defer root.close()
  fixture.write_b3_frame(root, "frame", 0, [])
  let frame = root.open_root(p"frame")?
  defer frame.close()
  let base = source.capture(frame, "profile", 1000000000, 4096, 100)?
  let previous = process_fixture.with_processes(
    base,
    "profile",
    1000000000,
    process_fixture.synthetic_processes(1000, 10000, 100000, process_fixture.process_seed),
  )
  let current = process_fixture.with_processes(
    base,
    "profile",
    2000000000,
    process_fixture.synthetic_processes(1000, 10000, 100100, process_fixture.process_seed),
  )
  let previous_2000 = process_fixture.synthetic_processes(2000, 10000, 100000, process_fixture.process_seed)
  let current_2000 = process_fixture.synthetic_processes(2000, 10000, 100100, process_fixture.process_seed)
  let _ = measured_process_join(current.processes.processes, previous.processes.processes)
  let _ = measured_process_join(current_2000, previous_2000)
  var samples_1000 = []
  var samples_2000 = []
  repeat 5 times {
    samples_1000 += [measured_process_join(current.processes.processes, previous.processes.processes)]
    samples_2000 += [measured_process_join(current_2000, previous_2000)]
  }
  let median_1000 = median(samples_1000)
  let median_2000 = median(samples_2000)
  let ratio = median_2000.float() / median_1000.float()
  print f"btop indexed process join scaling: seed={process_fixture.process_seed} repeats=5 median_1000_ns={median_1000} median_2000_ns={median_2000} ratio={ratio.format(precision: 2)}"
  assert median_1000 > 0 and median_2000 > 0
  assert ratio <= 3.0, f"indexed process update grew by {ratio.format(precision: 2)}x"
  let initial_model = model.step(previous, null)
  let initial_screen = renderer.render(initial_model, 120, 40)?
  let initial_surface = surface.from_lines(120, 40, initial_screen.lines)?
  let initial_update = surface.prepare(surface.initial(), initial_surface)
  var committed = surface.complete(initial_update, initial_update.bytes.byte_len())
  let samples = collect {
    repeat 20 times {
      let started_ns = time.monotonic()?
      let updated = model.step_cached(current, previous, model.empty_metadata_cache("profile"))
      let screen = renderer.render(updated.value, 120, 40)?
      let desired = surface.from_lines(120, 40, screen.lines)?
      let delta = surface.prepare(committed, desired)
      let written_bytes = delta.bytes.byte_len()
      committed = surface.complete(delta, written_bytes)
      yield time.monotonic()? - started_ns
    }
  }
  let ordered = samples |> sort
  let p95_ns = ordered[(ordered.len() * 95 + 99) / 100 - 1]
  let p99_ns = ordered[(ordered.len() * 99 + 99) / 100 - 1]
  print f"btop preloaded 1k model+render+diff: seed={process_fixture.process_seed} runner=Alpine_x86_64_musl_release size=120x40 samples={samples.len()} p95_ns={p95_ns} p99_ns={p99_ns}"
}

main()

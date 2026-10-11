use core.tests.support.btop as fixture

type CpuStateDump = {state: Str}
type ModelDump = {at_ns: Int, cpu_percent: CpuStateDump}
type ScreenDump = {width: Int, height: Int, lines: List[Str]}

test test_btop_cli_rejects_unknown_options { |ctx|
  let source = fp"{ctx.core_dir}/btop.xsh".read_text()?
  let output = test.run_xsh(
    ctx,
    source,
    [],
    ["--not-an-option"],
    env: {XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}"},
    name: "btop.xsh",
  )?

  assert output.status == 2
  assert "unrecognized option '--not-an-option'" in output.stderr
}

test test_btop_headless_replay_dumps_requested_frames_without_a_terminal { |ctx|
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let bundle_path = bundle.host_path()?
  let model_path = fp"{bundle_path}/model.jsonl"
  let screen_path = fp"{bundle_path}/screen.jsonl"
  let source = fp"{ctx.core_dir}/btop.xsh".read_text()?
  let output = test.run_xsh(
    ctx,
    source,
    [],
    ["--replay", bundle_path.display(), "--headless", "--frames", "1", "--dump-model", model_path.display(), "--dump-screen", screen_path.display()],
    env: {XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}"},
    name: "btop.xsh",
  )?

  assert output.success, output.stderr
  assert output.stdout == ""
  let model_lines = bundle.read_text(p"model.jsonl")?.split("\n")
  let screen_lines = bundle.read_text(p"screen.jsonl")?.split("\n")
  assert model_lines.len() == 2 and model_lines[1] == ""
  assert screen_lines.len() == 2 and screen_lines[1] == ""
  let model_frame = json.decode(model_lines[0])?.require(ModelDump)?
  let screen_frame = json.decode(screen_lines[0])?.require(ScreenDump)?
  assert model_frame.at_ns == 1000000000
  assert model_frame.cpu_percent.state == "warming"
  assert screen_frame.width == 80
  assert screen_frame.height == 24
}

test test_btop_headless_replay_syscall_trace_has_no_child_or_sleep { |ctx|
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let source = fp"{ctx.core_dir}/btop.xsh".read_text()?
  let output = test.run_xsht_trace(
    ctx,
    source,
    ["--syscalls", "--trace-top-syscalls", "128"],
    ["--replay", bundle.host_path()?.display(), "--headless"],
    env: {XSH_MODULE_PATH: f"{ctx.core_dir}:{ctx.core_dir.parent()}"},
    name: "btop.xsh",
  )?

  assert output.success, output.stderr
  assert "syscall summary" in output.stderr
  assert ! ("execve" in output.stderr)
  assert ! ("fork" in output.stderr)
  assert ! ("vfork" in output.stderr)
  assert ! ("clock_nanosleep" in output.stderr)
  assert ! ("nanosleep" in output.stderr)
}

test test_btop_live_source_syscall_trace_has_no_child_process { |ctx|
  let source = """
use core.lib.btop_source as source
let root = fs.open_root(/)?
defer root.close()
let units = system.execution_units()?
let observation = source.capture(root, "live-trace", time.monotonic()?, units.page_size_bytes, units.clock_ticks_per_second)?
assert observation.source_id == "live-trace"
"""
  let output = test.run_xsht_trace(
    ctx,
    source,
    ["--syscalls", "--trace-top-syscalls", "128"],
    [],
    env: {XSH_MODULE_PATH: ctx.core_dir.parent()},
    name: "btop-live-source-trace.xsh",
  )?

  assert output.success, output.stderr
  assert "syscall summary" in output.stderr
  assert ! ("execve" in output.stderr)
  assert ! ("fork" in output.stderr)
  assert ! ("vfork" in output.stderr)
}

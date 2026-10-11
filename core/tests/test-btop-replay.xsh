use core.lib.btop_model as model
use core.lib.btop_replay as replay
use core.lib.btop_rates as rates
use core.lib.system_report as report
use core.tests.support.btop as fixture

pure default_processes() -> List[fixture.ProcessFixture] {
  [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}]
}

test test_btop_replay_is_deterministic_and_uses_the_shared_model {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, default_processes())
  let bundle_path = bundle.host_path()?

  let loaded = replay.load(bundle_path)?
  let repeated = replay.load(bundle_path)?
  let first = replay.models(loaded.observations)
  let second = replay.models(repeated.observations)
  assert loaded.display.term == "xterm-256color"
  assert loaded.display.rows == 24 and loaded.display.cols == 80
  assert first.len() == 2
  assert first[0].cpu_percent.state == rates.Warming
  assert first[1].cpu_percent.value?.format(precision: 1) == "40.0"
  assert first[1].processes[0].cpu_percent.value?.format(precision: 1) == "12.5"
  assert first[1].memory.used_bytes.value == 805306368
  assert first[1].network.rx_bytes_per_second.value == 10240.0
  assert first[1].network.tx_bytes_per_second.value == 2048.0
  assert first[1].cpu_percent == second[1].cpu_percent
  assert first[1].processes == second[1].processes
}

test test_btop_b3_fixture_raw_sources_match_frozen_digests {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )

  assert bundle.read_bytes(p"frames/0/proc/stat")?.sha256().hex() == "b6bcdd13221c478f39e051763fcb93d8c9b7dc697afe08e291e3c7811bc80a2c"
  assert bundle.read_bytes(p"frames/1/proc/stat")?.sha256().hex() == "1b20533e6116efcc1140f72f6d12163c9cca7825254224d2df0c714e4dac331e"
  assert bundle.read_bytes(p"frames/0/proc/meminfo")?.sha256().hex() == "5eb49555bb8760e441cb32d6be6ed9dfb7db207bba7a5ba9942b0d644d407a37"
  assert bundle.read_bytes(p"frames/0/proc/net/dev")?.sha256().hex() == "109413ba19cda5adaf5d86b6d0b2846695db0450b3384865cd8e3bc94bf9a7b8"
  assert bundle.read_bytes(p"frames/1/proc/net/dev")?.sha256().hex() == "5a57468231108652c6f00d64ac3075c83c9a143b0930cd1f06585eebaf6dfa26"
  assert bundle.read_bytes(p"frames/0/sys/class/net/eth0/ifindex")?.sha256().hex() == "53c234e5e8472b6ac51c1ae1cab3fe06fad053beb8ebfd8977b010655bfdd3c3"
  assert bundle.read_bytes(p"frames/0/proc/42/stat")?.sha256().hex() == "b88086dd0d70c6b9b44cfda6ee07f83dc4bb5882b86652bdc6c47c2b5d4395ca"
  assert bundle.read_bytes(p"frames/1/proc/42/stat")?.sha256().hex() == "f212e62d305d16e8487e75583e552e5df512b389753db23480d74446df225002"
}

test test_btop_replay_never_falls_back_to_missing_host_sources {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, default_processes())
  bundle.remove(p"frames/1/proc/stat")
  let loaded = replay.load(bundle.host_path()?)?
  let observations = loaded.observations
  let frames = replay.models(observations)

  assert observations[1].cpu.stat.state == report.Absent
  assert frames[1].cpu_state == report.Absent
  assert frames[1].cpu_percent.state == rates.Unavailable
}

test test_btop_replay_rejects_nonincreasing_frame_times {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, default_processes())
  bundle.write(
    p"manifest.json",
    """{"schema":2,"page_size_bytes":4096,"clock_ticks_per_second":100,"source_id":"bad-time","display":{"term":"xterm","colorterm":"","no_color":false,"lc_all":"C.UTF-8","lc_ctype":"","lang":"","rows":24,"cols":80},"frames":[{"at_ns":2000000000},{"at_ns":2000000000}]}\n""",
  )
  test.error_kind(replay.load(bundle.host_path()?), "validation")
}

test test_btop_replay_rejects_the_g1_manifest_schema_without_display_inputs {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, default_processes())
  bundle.write(
    p"manifest.json",
    """{"schema":1,"page_size_bytes":4096,"clock_ticks_per_second":100,"source_id":"legacy","frames":[{"at_ns":1000000000}]}\n""",
  )
  test.error_kind(replay.load(bundle.host_path()?), "validation")
}

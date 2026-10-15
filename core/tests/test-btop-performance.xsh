use core.lib.btop_disks as disks
use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.lib.btop_render as renderer
use core.lib.btop_source as source
use core.lib.btop_surface as surface
use core.lib.sys_block as block
use core.lib.sys_mount as mounts
use core.lib.sys_proc as sysproc
use core.lib.system_report as report
use core.tests.support.btop as fixture
use core.tests.support.btop_process_fixture as process_fixture

const process_count = 256
const disk_count = 64

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

proc write_storage_frame(root: FsRoot) [fs, error] {
  fixture.write_b3_frame(root, "frame", 0, [])
  root.mkdir(p"frame/sys/class/block", parents: true)
  root.mkdir(p"frame/sys/devices/virtual/block/dm-0/holders", parents: true)
  root.mkdir(p"frame/sys/devices/virtual/block/dm-0/slaves", parents: true)
  root.mkdir(p"frame/sys/devices/virtual/block/dm-0/queue", parents: true)
  root.write(p"frame/sys/devices/virtual/block/dm-0/dev", "253:0\n")
  root.write(p"frame/sys/devices/virtual/block/dm-0/diskseq", "1\n")
  root.write(p"frame/sys/devices/virtual/block/dm-0/size", "4096\n")
  root.write(p"frame/sys/devices/virtual/block/dm-0/stat", "1 0 10 0 1 0 20 0 0 0 0\n")
  root.symlink(target: p"../../devices/virtual/block/dm-0", path: p"frame/sys/class/block/dm-0")
  root.mkdir(p"frame/mnt/disk", parents: true)
  root.mkdir(p"frame/proc/self", parents: true)
  root.write(p"frame/proc/self/mountinfo", "1 0 253:0 / /mnt/disk rw - ext4 /dev/dm-0 rw\n")
}

pure disk_epoch(base: disks.Source, count: Int, epoch: Int, counter_base: Int) -> disks.Source {
  let template_device = base.blocks.devices[0]
  let template_mount = base.mounts.mounts[0]
  let devices: List[block.BlockDevice] = collect {
    for index in range(count) {
      let io_counters = [
        {...counter, value: counter_base + index}
        for counter in template_device.io_counters
      ]
      yield {
        ...template_device,
        name: f"dm-{index}",
        major: 253,
        minor: index,
        disk_sequence: epoch + index,
        disk_sequence_state: report.Observed,
        io_counters: io_counters,
      }
    }
  }
  let mount_rows: List[mounts.MountEntry] = collect {
    for index in range(count) {
      yield {
        ...template_mount,
        mount_id: 1000 + index,
        parent_id: 0,
        major: 253,
        minor: index,
        target: f"/mnt/disk-{index}",
      }
    }
  }
  {...base, blocks.devices: devices, mounts.mounts: mount_rows}
}

test test_btop_process_join_scales_from_one_thousand_to_two_thousand {
  let root = fs.tempdir()?
  defer root.close()
  fixture.write_b3_frame(root, "frame", 0, [])
  let frame = root.open_root(p"frame")?
  defer frame.close()
  let base = source.capture(frame, "scaling", 1000000000, 4096, 100)?
  let previous_1000 = process_fixture.with_processes(
    base,
    "scaling",
    1000000000,
    process_fixture.synthetic_processes(1000, 10000, 100000, process_fixture.process_seed),
  )
  let current_1000 = process_fixture.with_processes(
    base,
    "scaling",
    2000000000,
    process_fixture.synthetic_processes(1000, 10000, 100100, process_fixture.process_seed),
  )
  let previous_2000 = process_fixture.with_processes(
    base,
    "scaling",
    1000000000,
    process_fixture.synthetic_processes(2000, 10000, 100000, process_fixture.process_seed),
  )
  let current_2000 = process_fixture.with_processes(
    base,
    "scaling",
    2000000000,
    process_fixture.synthetic_processes(2000, 10000, 100100, process_fixture.process_seed),
  )
  let joined_1000 = model.process_join(current_1000.processes.processes, previous_1000.processes.processes)
  assert joined_1000.len() == 1000
  assert joined_1000 |> all .previous != null
  assert joined_1000[0].current.pid == 10000 and joined_1000[0].previous?.pid == 10000
  let reused = {...current_1000.processes.processes[0], start_ticks: 999999}
  assert model.process_join([reused], previous_1000.processes.processes)[0].previous == null

  let _ = measured_process_join(current_1000.processes.processes, previous_1000.processes.processes)
  let _ = measured_process_join(current_2000.processes.processes, previous_2000.processes.processes)
  var samples_1000 = []
  var samples_2000 = []
  repeat 5 times {
    samples_1000 += [measured_process_join(current_1000.processes.processes, previous_1000.processes.processes)]
    samples_2000 += [measured_process_join(current_2000.processes.processes, previous_2000.processes.processes)]
  }
  let median_1000 = median(samples_1000)
  let median_2000 = median(samples_2000)
  let ratio = median_2000.float() / median_1000.float()
  print f"btop indexed process join scaling: seed={process_fixture.process_seed} repeats=5 median_1000_ns={median_1000} median_2000_ns={median_2000} ratio={ratio.format(precision: 2)}"
  assert median_1000 > 0 and median_2000 > 0
  assert ratio <= 3.0, f"indexed process update grew by {ratio.format(precision: 2)}x"
}

test test_btop_process_and_device_identity_churn_stays_bounded_and_warms_new_epochs {
  let root = fs.tempdir()?
  defer root.close()
  write_storage_frame(root)
  let frame = root.open_root(p"frame")?
  defer frame.close()
  let base = source.capture(frame, "churn", 1000000000, 4096, 100)?
  var previous: model.Observation? = null
  var cache = model.empty_metadata_cache("churn")
  var first_pid = 0

  for cycle in range(8) {
    let current_processes = process_fixture.synthetic_processes(
      process_count,
      10000 + cycle * 1000,
      100000 + cycle * 100,
      process_fixture.process_seed + cycle,
    )
    let current: model.Observation = model.Observation(
      source_id: "churn",
      at_ns: (cycle + 1) * 1000000000,
      page_size_bytes: base.page_size_bytes,
      clock_ticks_per_second: base.clock_ticks_per_second,
      cpu: base.cpu,
      memory: base.memory,
      network: base.network,
      processes: {...base.processes, processes: current_processes},
      telemetry: base.telemetry,
      storage: disk_epoch(base.storage, disk_count, cycle * 1000 + 10, cycle * 100),
    )
    let updated = model.step_cached(current, previous, cache)
    assert updated.value.processes.len() == process_count
    assert updated.metadata.by_pid.len() == process_count
    assert updated.value.storage.devices.len() == disk_count
    if cycle == 0 {
      first_pid = current_processes[0].pid
    } else {
      assert first_pid not in updated.metadata.by_pid
      assert updated.value.processes[0].cpu_percent.state == .Warming
      assert updated.value.storage.devices[0].read_bytes_per_second.state == .Warming
    }
    previous = current
    cache = updated.metadata
  }
}

test test_btop_optional_source_loss_keeps_process_view_and_labels_unavailable_fields {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(
    bundle,
    "frame",
    0,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  bundle.remove(p"frame/proc/meminfo")
  bundle.remove(p"frame/proc/net/dev")
  let frame = bundle.open_root(p"frame")?
  defer frame.close()
  let observation = source.capture(frame, "degraded", 1000000000, 4096, 100)?
  let value = model.step(observation, null)
  let screen = renderer.render(value, 80, 24)?
  let visible = screen.lines.join("\n")

  assert value.process_state == .Observed
  assert value.processes.len() == 1 and value.processes[0].pid == 42
  assert value.memory.total_state == .Absent and value.memory.total_bytes == null
  assert value.network.source_state == .Absent and value.network.interface_name == null
  assert value.storage.block_state == .Absent and value.storage.mount_state == .Absent
  assert "Total absent" in visible
  assert "Interface absent" in visible
  assert "Temperature absent" in visible
  assert "Frequency absent" in visible
  assert "Battery absent" in visible
}

proc sampling_fixture(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "linux-sampling")?
  fp"{root}/stat".write("""cpu 100 2 30 400 5 6 7 8 9 10
ctxt 900
processes 40
procs_running 2
procs_blocked 1
intr 777 1 2
btime 100
""")
  fp"{root}/uptime".write("123.45 67.89\n")
  fp"{root}/vmstat".write("""pgpgin 11
pgpgout 12
pswpin 13
pswpout 14
""")
  fp"{root}/meminfo".write("""MemTotal: 1024 kB
MemFree: 100 kB
MemAvailable: 800 kB
Buffers: 20 kB
Cached: 30 kB
Shmem: 40 kB
SReclaimable: 50 kB
SwapTotal: 64 kB
SwapFree: 60 kB
""")
  fp"{root}/diskstats".write("8 0 sda 1 2 3 4 5 6 7 8 9 10 11 12 13\n8 1 sda1 20 21 22 23 24 25 26 27 28 29 30\n")
  fp"{root}/123".mkdir()
  fp"{root}/123/stat".write("123 (worker ) with spaces) S 1 42 43 34821 0 0 0 0 0 0 17 19 0 0 20 -3 4 0 1250 65536 7 0 0 0 0 0 0 0 0 0 0 0 0 0 0 3\n")
  fp"{root}/123/cmdline".write(b"worker\0argument\0")
  root
}

test test_linux_sample_decodes_shared_process_cpu_memory_and_disk_counters { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("Linux sampling requires Linux")
    return
  }
  let sample = linux.sample(proc_root: sampling_fixture(ctx)?)?
  assert sample.uptime_ms == 123450
  assert sample.sampled_at_ms >= 0
  assert sample.ticks_per_second > 0
  assert sample.page_size > 0
  assert sample.cpu.user == 100 and sample.cpu.idle == 400
  assert sample.cpu.iowait == 5 and sample.cpu.steal == 8
  assert sample.context_switches == 900 and sample.interrupts == 777
  assert sample.running == 2 and sample.blocked == 1
  assert sample.processes_created == 40
  assert sample.page_in_kib == 11 and sample.page_out_kib == 12
  assert sample.swap_in_pages == 13 and sample.swap_out_pages == 14
  assert sample.memory.total == 1048576
  assert sample.memory.shared == 40960 and sample.memory.sreclaimable == 51200
  let first = sample.processes[0]
  assert first.pid == 123 and first.parent_pid == 1
  assert first.command == "worker ) with spaces"
  assert first.argv == "worker argument"
  assert first.user_ticks == 17 and first.system_ticks == 19 and first.cpu_ticks == 36
  assert first.start_ticks == 1250
  assert first.start_time_ms == 100000 + 1250000 / sample.ticks_per_second
  assert first.pgrp == 42 and first.session == 43
  assert first.tty_number == 34821 and first.tty == "pts/5"
  assert first.nice == -3 and first.thread_count == 4 and first.processor == 3
  assert first.rss_bytes == 7 * sample.page_size and first.vsize_bytes == 65536
  let disk = sample.disks[0]
  assert disk.name == "sda" and disk.major == 8 and disk.minor == 0
  assert disk.reads_completed == 1 and disk.reads_merged == 2
  assert disk.sectors_read == 3 and disk.read_ms == 4
  assert disk.writes_completed == 5 and disk.writes_merged == 6
  assert disk.sectors_written == 7 and disk.write_ms == 8
  assert disk.in_flight == 9 and disk.io_ms == 10 and disk.weighted_io_ms == 11
}

test test_linux_sample_rejects_missing_and_malformed_required_counters { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("Linux sampling requires Linux")
    return
  }
  let root = sampling_fixture(ctx)?
  fp"{root}/vmstat".write("pgpgin 11\npgpgout 12\npswpin 13\n")
  test.error_kind(linux.sample(proc_root: root), "linux-sample")
  fp"{root}/vmstat".write("pgpgin 11\npgpgout 12\npswpin 13\npswpout nope\n")
  test.error_kind(linux.sample(proc_root: root), "linux-sample")
  fp"{root}/vmstat".write("pgpgin 11\npgpgout 12\npswpin 13\npswpout 14\n")
  fp"{root}/123/stat".write("123 (truncated) S 1\n")
  test.error_kind(linux.sample(proc_root: root), "linux-sample")
  fp"{root}/123/stat".remove()
  fp"{root}/diskstats".write("8 0 broken 1 2\n")
  test.error_kind(linux.sample(proc_root: root), "linux-sample")
}

test test_linux_sample_host_and_process_inventory_share_counter_fields {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("Linux sampling requires Linux")
    return
  }
  let sample = linux.sample()?
  assert sample.uptime_ms >= 0
  assert sample.memory.shared >= 0 and sample.memory.sreclaimable >= 0
  let current_pid = process.current_pid()?
  assert sample.processes |> any .pid == current_pid
  var found = false
  for entry in process.list()? {
    if entry.pid == current_pid {
      found = true
      assert (entry.cpu_ticks ?? -1) >= 0
      assert (entry.rss_bytes ?? -1) >= 0
      assert (entry.pgrp ?? -1) >= 0
      assert entry.tty != null
    }
  }
  assert found
}

test test_linux_open_files_labels_process_references_and_descriptor_access {
  guard system.uname()?.sysname == "Linux" else {
    test.skip("Linux descriptor metadata requires Linux")
    return
  }
  let files = linux.open_files(process.current_pid()?)?.collect()
  assert files |> any .fd_label == "cwd"
  assert files |> any .fd_label == "rtd"
  assert files |> any .fd_label == "txt"
  let cwd = fs.cwd()?
  let metadata = fs.stat(cwd)?
  for file in files {
    if file.fd_label == "cwd" {
      assert file.path == cwd
      assert file.dev == metadata.dev and file.inode == metadata.ino
    }
    if file.fd >= 0 {
      assert file.fd_label == f"{file.fd}"
      assert file.access in ["r", "w", "u", "?"]
    } else {
      assert file.access == ""
    }
  }
}

##! Complete raw procfs fixtures for btop model, replay, and entrypoint tests.
##!
##! The frame contents are fixed at the kernel-file boundary so expected rates
##! remain independent literals in the tests that consume them.

## Describes a complete process record for a captured procfs frame.
export type ProcessFixture = {
  pid: Int,
  start_ticks: Int,
  cpu_ticks: Int,
  rss_pages: Int,
  uid: Int,
}

proc process_stat(pid: Int, start_ticks: Int, cpu_ticks: Int, rss_pages: Int) -> Str {
  f"{pid} (sample worker) S 1 {pid} {pid} 0 -1 4194560 100 0 0 0 {cpu_ticks} 0 0 0 20 1 1 0 {start_ticks} 16777216 {rss_pages} 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"
}

## Writes a valid two-CPU, memory, network, and process source set under prefix.
export proc write_b3_frame(root: FsRoot, prefix: Str, index: Int, process_rows: List[ProcessFixture]) [fs, error] {
  let frame = fp"{prefix}/proc"
  root.mkdir(fp"{frame}/net", parents: true)
  let aggregate = if index == 0 {
    "cpu 100 0 50 850 0 0 0 0 0 0"
  } else {
    "cpu 160 0 70 970 0 0 0 0 0 0"
  }
  let cpu0 = if index == 0 {
    "cpu0 50 0 25 425 0 0 0 0 0 0"
  } else {
    "cpu0 80 0 35 485 0 0 0 0 0 0"
  }
  let cpu1 = cpu0.replace("cpu0", with: "cpu1")
  root.write(
    fp"{frame}/stat",
    f"{aggregate}\n{cpu0}\n{cpu1}\nintr 1\nctxt 2\nbtime 1700000000\nprocesses 3\nprocs_running 1\nprocs_blocked 0\nsoftirq 4 0 0 0 0 0 0 0 0 0 0\n",
  )
  root.write(fp"{frame}/uptime", "25.50 100.00\n")
  root.write(fp"{frame}/loadavg", "0.10 0.20 0.30 1/100 42\n")
  root.write(
    fp"{frame}/meminfo",
    "MemTotal: 1048576 kB\nMemFree: 131072 kB\nMemAvailable: 262144 kB\nBuffers: 32768 kB\nCached: 65536 kB\nActive: 524288 kB\nInactive: 131072 kB\nDirty: 128 kB\nWriteback: 0 kB\n",
  )
  let receive = if index == 0 { 1000000 } else { 1010240 }
  let transmit = if index == 0 { 500000 } else { 502048 }
  root.write(
    fp"{frame}/net/dev",
    f"Inter-| Receive                                                | Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\neth0: {receive} 100 0 0 0 0 0 0 {transmit} 100 0 0 0 0 0 0\nlo: 64 1 0 0 0 0 0 0 64 1 0 0 0 0 0 0\n",
  )
  root.mkdir(fp"{prefix}/sys/class/net/eth0", parents: true)
  root.write(fp"{prefix}/sys/class/net/eth0/ifindex", "2\n")
  root.mkdir(fp"{prefix}/sys/class/net/lo", parents: true)
  root.write(fp"{prefix}/sys/class/net/lo/ifindex", "1\n")
  for row in process_rows {
    let directory = fp"{frame}/{row.pid}"
    root.mkdir(directory, parents: true)
    root.write(fp"{directory}/stat", process_stat(row.pid, row.start_ticks, row.cpu_ticks, row.rss_pages))
    root.write(fp"{directory}/statm", f"65536 {row.rss_pages} 100 0 0 1 0\n")
    root.write(fp"{directory}/status", f"Name:\tsample worker\nUid:\t{row.uid}\t{row.uid}\t{row.uid}\t{row.uid}\n")
  }
}

## Creates both frames and their strict replay manifest in a private bundle.
export proc write_b3_bundle(root: FsRoot, process_rows: List[ProcessFixture]) [fs, error] {
  write_b3_frame(root, "frames/0", 0, process_rows)
  write_b3_frame(
    root,
    "frames/1",
    1,
    [
      {
        pid: row.pid,
        start_ticks: row.start_ticks,
        cpu_ticks: row.cpu_ticks + 25,
        rss_pages: row.rss_pages,
        uid: row.uid,
      }
      for row in process_rows
    ],
  )
  root.write(
    p"manifest.json",
    """{"schema":2,"page_size_bytes":4096,"clock_ticks_per_second":100,"source_id":"b3-fixture","display":{"term":"xterm-256color","colorterm":"","no_color":false,"lc_all":"C.UTF-8","lc_ctype":"","lang":"","rows":24,"cols":80},"frames":[{"at_ns":1000000000},{"at_ns":2000000000}]}\n""",
  )
}

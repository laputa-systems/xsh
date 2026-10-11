##! Adapts one caller-owned rooted tree into the monitor's raw observations.
##!
##! Page size, clock tick rate, source identity, and monotonic timestamp are
##! explicit inputs because a filesystem root cannot prove any of them.
use btop_model as model
use btop_disks as disks
use sys_cpu
use sys_mem
use sys_net
use sys_proc
use sys_mount
use system_report_live as system_live

## Reads counter sources and collects or reuses gauge telemetry from one tree.
export proc capture(
  root: FsRoot,
  source_id: Str,
  at_ns: Int,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
  cached_telemetry: system_live.MonitorSources? = null,
  cached_mounts: sys_mount.MountTable? = null,
  cached_mounts_at_ns: Int? = null,
  captured_mount_usage: List[sys_mount.CapturedUsage]? = null,
) [fs, error] -> Result[model.Observation, Error] {
  if at_ns < 0 or page_size_bytes <= 0 or clock_ticks_per_second <= 0 {
    return Err(error.failure("btop source metadata must be nonnegative and rates must be positive"))
  }

  Ok({
    source_id: source_id,
    at_ns: at_ns,
    page_size_bytes: page_size_bytes,
    clock_ticks_per_second: clock_ticks_per_second,
    cpu: sys_cpu.collect(root),
    memory: sys_mem.collect(root),
    network: sys_net.collect(root),
    processes: sys_proc.collect(root, page_size_bytes),
    telemetry: if cached_telemetry == null { system_live.collect_monitor_sources(root) } else { cached_telemetry ?? system_live.collect_monitor_sources(root) },
    storage: disks.collect(
      root,
      at_ns,
      cached_mounts: cached_mounts,
      cached_mounts_at_ns: cached_mounts_at_ns,
      captured_usage: captured_mount_usage,
    )?,
  })
}

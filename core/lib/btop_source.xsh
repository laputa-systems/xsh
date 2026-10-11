##! Adapts one caller-owned rooted tree into the monitor's raw observations.
##!
##! Page size, clock tick rate, source identity, and monotonic timestamp are
##! explicit inputs because a filesystem root cannot prove any of them.
use btop_model as model
use sys_cpu
use sys_mem
use sys_net
use sys_proc

## Reads the four monitor sources from one observation tree.
export proc capture(
  root: FsRoot,
  source_id: Str,
  at_ns: Int,
  page_size_bytes: Int,
  clock_ticks_per_second: Int,
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
  })
}

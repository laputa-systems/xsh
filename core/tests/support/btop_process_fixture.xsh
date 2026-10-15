##! Deterministic process observations shared by btop behavior and profile tests.
use core.lib.btop_model as model
use core.lib.sys_proc as sysproc

## The fixed seed for generated btop process fixtures.
export const process_seed = 424242

## Produces count process records starting at first_pid with deterministic values from seed.
export pure synthetic_processes(count: Int, first_pid: Int, cpu_base: Int, seed: Int) -> List[sysproc.Process] {
  var state = seed
  collect {
    for index in range(count) {
      state = (state * 1103515245 + 12345) % 2147483648
      let pid = first_pid + index
      yield {
        pid: pid,
        parent_pid: 1,
        start_ticks: 100 + index,
        command: "seeded-worker",
        thread_count: 1,
        uid: 1000,
        cpu_ticks: cpu_base + state % 1024,
        rss_bytes: 4096 * (1 + state % 4096),
      }
    }
  }
}

## Replaces the source identity, timestamp, and process rows on a base observation.
export pure with_processes(
  base: model.Observation,
  source_id: Str,
  at_ns: Int,
  rows: List[sysproc.Process],
) -> model.Observation {
  {...base, source_id: source_id, at_ns: at_ns, processes.processes: rows}
}

use core.lib.btop_process_tree as tree
use core.lib.btop_model as model
use core.lib.btop_rates as rates

pure process_row(pid: Int, parent_pid: Int, start_ticks: Int, cpu_rate: rates.Rate, rss_bytes: Int?) -> model.ProcessRow {
  {
    pid: pid,
    parent_pid: parent_pid,
    start_ticks: start_ticks,
    thread_count: 1,
    uid: 1000,
    uid_state: model.Fresh,
    command: f"process-{pid}",
    cpu_percent: cpu_rate,
    cpu_percent_per_core: cpu_rate,
    cpu_average_percent: cpu_rate,
    rss_bytes: rss_bytes,
  }
}

pure observed_cpu(value: Float) -> rates.Rate {
  {state: rates.Observed, value: value}
}

test test_btop_process_tree_repairs_orphan_self_parent_cycle_and_reused_parent_edges {
  let result = tree.build([
    process_row(1, 0, 10, observed_cpu(1.0), 100),
    process_row(2, 1, 11, observed_cpu(2.0), 20),
    process_row(3, 2, 12, observed_cpu(3.0), 10),
    process_row(4, 99, 13, observed_cpu(4.0), 5),
    process_row(5, 5, 14, observed_cpu(5.0), 7),
    process_row(6, 7, 20, observed_cpu(6.0), 6),
    process_row(7, 6, 20, observed_cpu(7.0), 7),
    process_row(8, 9, 25, observed_cpu(8.0), 8),
    process_row(9, 0, 26, observed_cpu(9.0), 9),
  ])

  assert result.roots == [1, 4, 5, 7, 8, 9]
  assert result.preorder == [1, 2, 3, 4, 5, 7, 6, 8, 9]
  assert result.nodes[1].children == [2]
  assert result.nodes[2].children == [3]
  assert result.nodes[3].parent_pid == 2
  assert result.nodes[4].orphan
  assert result.nodes[5].parent_pid == null
  assert result.nodes[7].cycle_break and result.nodes[7].parent_pid == null
  assert result.nodes[6].parent_pid == 7
  assert result.nodes[8].orphan
  assert result.nodes[8].parent_pid == null
  assert result.issues.len() == 4

  let reversed = tree.build([
    process_row(9, 0, 26, observed_cpu(9.0), 9),
    process_row(8, 9, 25, observed_cpu(8.0), 8),
    process_row(7, 6, 20, observed_cpu(7.0), 7),
    process_row(6, 7, 20, observed_cpu(6.0), 6),
    process_row(5, 5, 14, observed_cpu(5.0), 7),
    process_row(4, 99, 13, observed_cpu(4.0), 5),
    process_row(3, 2, 12, observed_cpu(3.0), 10),
    process_row(2, 1, 11, observed_cpu(2.0), 20),
    process_row(1, 0, 10, observed_cpu(1.0), 100),
  ])
  assert reversed.preorder == result.preorder
}

test test_btop_process_tree_aggregates_descendants_once_and_withholds_incomplete_rates {
  let rows = [
    process_row(10, 0, 10, observed_cpu(1.0), 100),
    process_row(11, 10, 11, observed_cpu(2.0), 20),
    process_row(12, 11, 12, observed_cpu(3.0), 10),
  ]
  let ordinary = tree.build(rows)
  let aggregate = tree.build(rows, aggregate_children: true)
  assert ordinary.nodes[10].cpu_total.value == 1.0
  assert ordinary.nodes[10].rss_total_bytes == 100
  assert aggregate.nodes[10].cpu_total.value == 6.0
  assert aggregate.nodes[10].rss_total_bytes == 130
  assert aggregate.nodes[11].cpu_total.value == 5.0
  assert aggregate.nodes[11].rss_total_bytes == 30
  assert aggregate.nodes[12].cpu_total.value == 3.0
  assert aggregate.nodes[12].rss_total_bytes == 10

  let incomplete = tree.build([
    process_row(10, 0, 10, observed_cpu(1.0), 100),
    process_row(11, 10, 11, {state: rates.Warming, value: null}, 20),
  ], aggregate_children: true)
  assert incomplete.nodes[10].cpu_total.state == rates.Unavailable
  assert incomplete.nodes[10].cpu_total.value == null
}

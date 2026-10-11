##! Builds safe process adjacency and resource totals without recursive walks.
##!
##! Every node belongs to the current sample; missing or inconsistent parents
##! become roots, and cycle repair is deterministic for replayable snapshots.
use btop_model as model
use btop_rates as rates

## Describes a process-tree inconsistency without hiding the affected process.
export type TreeIssue = {pid: Int, kind: Str}

## Retains one process, its repaired parent relation, and its current children.
export type TreeNode = {
  row: model.ProcessRow,
  parent_pid: Int?,
  children: List[Int],
  depth: Int,
  orphan: Bool,
  cycle_break: Bool,
  cpu_total: rates.Rate,
  rss_total_bytes: Int?,
}

## Holds deterministic roots, adjacency, preorder, and graph repair evidence.
export type ProcessTree = {
  roots: List[Int],
  nodes: Map[Int, TreeNode],
  preorder: List[Int],
  issues: List[TreeIssue],
}

type PendingNode = {pid: Int, depth: Int}

pure process_index(rows: List[model.ProcessRow]) -> Map[Int, model.ProcessRow] {
  var by_pid: Map[Int, model.ProcessRow] = {}
  for row in rows {
    if row.pid > 0 and row.pid not in by_pid {
      by_pid = by_pid.set(row.pid, row)
    }
  }
  by_pid
}

pure add_rate(left: rates.Rate, right: rates.Rate) -> rates.Rate {
  if left.state != rates.Observed or right.state != rates.Observed or left.value == null or right.value == null {
    return {state: rates.Unavailable, value: null}
  }
  {state: rates.Observed, value: (left.value ?? 0.0) + (right.value ?? 0.0)}
}

pure add_bytes(left: Int?, right: Int?) -> Int? {
  if left == null or right == null { return null }
  let a = left ?? 0
  let b = right ?? 0
  return null when a < 0 or b < 0 or a > 9223372036854775807 - b
  a + b
}

## Builds one adjacency index, cutting self-links, reused-parent edges, and cycles.
export pure build(rows: List[model.ProcessRow], aggregate_children: Bool = false) -> ProcessTree {
  let ordered = rows |> sort-by .pid
  let by_pid = process_index(ordered)
  var parent_by_pid: Map[Int, Int] = {}
  var orphan_by_pid: Map[Int, Bool] = {}
  var issues: List[TreeIssue] = []
  var seen_rows: Set[Int] = set.empty()
  for row in ordered {
    if row.pid <= 0 or row.pid in seen_rows {
      issues += [{pid: row.pid, kind: "duplicate_or_invalid_pid"}]
      continue
    }
    seen_rows = seen_rows.add(row.pid)
    if row.parent_pid == 0 { continue }
    if row.parent_pid == row.pid {
      issues += [{pid: row.pid, kind: "self_parent"}]
      continue
    }
    if let Ok(parent) = by_pid.get(row.parent_pid) {
      if parent.start_ticks > row.start_ticks {
        orphan_by_pid = orphan_by_pid.set(row.pid, true)
        issues += [{pid: row.pid, kind: "parent_started_after_child"}]
      } else {
        parent_by_pid = parent_by_pid.set(row.pid, row.parent_pid)
      }
    } else {
      orphan_by_pid = orphan_by_pid.set(row.pid, true)
      issues += [{pid: row.pid, kind: "missing_parent"}]
    }
  }

  var cycle_breaks: Set[Int] = set.empty()
  var finished: Set[Int] = set.empty()
  for row in ordered {
    continue when row.pid in finished
    var cursor = row.pid
    var predecessor: Int? = null
    var walked: List[Int] = []
    var path_index: Map[Int, Int] = {}
    for _ in range(ordered.len() + 1) {
      if cursor in finished { break }
      if let Ok(_) = path_index.get(cursor) {
        let broken = predecessor ?? cursor
        parent_by_pid = parent_by_pid.remove(broken)
        cycle_breaks = cycle_breaks.add(broken)
        issues += [{pid: broken, kind: "parent_cycle_broken"}]
        break
      }
      path_index = path_index.set(cursor, walked.len())
      walked += [cursor]
      if let Ok(parent) = parent_by_pid.get(cursor) {
        predecessor = cursor
        cursor = parent
      } else {
        break
      }
    }
    for pid in walked { finished = finished.add(pid) }
  }

  var roots: List[Int] = []
  var children_by_pid: Map[Int, List[Int]] = {}
  for row in ordered {
    if let Ok(parent) = parent_by_pid.get(row.pid) {
      let children = if let Ok(found) = children_by_pid.get(parent) { found } else { [] }
      children_by_pid = children_by_pid.set(parent, children + [row.pid])
    } else {
      roots += [row.pid]
    }
  }

  var nodes: Map[Int, TreeNode] = {}
  for row in ordered {
    var parent: Int? = null
    if let Ok(found) = parent_by_pid.get(row.pid) { parent = found }
    let children = if let Ok(found) = children_by_pid.get(row.pid) { found } else { [] }
    let orphan = if let Ok(found) = orphan_by_pid.get(row.pid) { found } else { false }
    nodes = nodes.set(row.pid, {
      row: row,
      parent_pid: parent,
      children: children,
      depth: 0,
      orphan: orphan,
      cycle_break: row.pid in cycle_breaks,
      cpu_total: row.cpu_percent,
      rss_total_bytes: row.rss_bytes,
    })
  }

  var stack: List[PendingNode] = []
  for index in range(roots.len()) {
    stack += [{pid: roots[roots.len() - index - 1], depth: 0}]
  }
  var preorder: List[Int] = []
  while ! stack.is_empty() {
    let pending = stack[stack.len() - 1]
    stack = stack |> take(stack.len() - 1)
    preorder += [pending.pid]
    if let Ok(node) = nodes.get(pending.pid) {
      nodes = nodes.set(pending.pid, {...node, depth: pending.depth})
      for index in range(node.children.len()) {
        stack += [{pid: node.children[node.children.len() - index - 1], depth: pending.depth + 1}]
      }
    }
  }

  if aggregate_children {
    for index in range(preorder.len()) {
      let pid = preorder[preorder.len() - index - 1]
      if let Ok(node) = nodes.get(pid) {
        var cpu_total = node.row.cpu_percent
        var rss_total = node.row.rss_bytes
        for child_pid in node.children {
          if let Ok(child) = nodes.get(child_pid) {
            cpu_total = add_rate(cpu_total, child.cpu_total)
            rss_total = add_bytes(rss_total, child.rss_total_bytes)
          }
        }
        nodes = nodes.set(pid, {...node, cpu_total: cpu_total, rss_total_bytes: rss_total})
      }
    }
  }

  {roots: roots, nodes: nodes, preorder: preorder, issues: issues}
}

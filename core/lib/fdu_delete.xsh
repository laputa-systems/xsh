##! Computes fdu's per-entry deletion plan and reports skipped entries.
##!
##! Deletion is permanent and per-entry: a directory is removed only when every
##! child was removed, so a skipped or excluded path survives with its parent.
use core.lib.fdu_model as model

## Names what a plan does with one node.
export enum Disposition { Retain, Enter, Remove }

## Pairs a node that cannot be deleted with its reason.
export type Skipped = {node: Int, reason: Str}

## Names one selection that cannot produce a plan.
export type Rejection = {node: Int?, reason: Str}

## A frozen plan: roots in mark order, targets in postorder, skipped entries.
export type Plan = {
  roots: List[Int],
  targets: List[Int],
  disposition: List[Disposition],
  skipped: List[Skipped],
  apparent: Int,
  allocated: Int,
}

## Returns either a plan or every reason the selection cannot be deleted.
export type PlanResult = {ok: Bool, plan: Plan, rejected: List[Rejection]}

## Describes one finished deletion.
export type Report = {
  deleted: Int,
  failed: Int,
  skipped: Int,
  messages: List[Str],
}

## Describes the tree and report left behind by deletion.
export type Executed = {tree: model.Tree, report: Report}

pure skip_reason(state: model.State) -> Str {
  match state {
    model.Mount => "mount boundary is protected"
    model.Alias => "unsupported directory alias is protected"
    model.Complete => "nothing inside it can be deleted"
    model.Pending => "scan is still in progress"
    model.Incomplete => "scan is incomplete"
    model.Stale => "entry totals are stale; rescan first"
    model.Tombstone => "entry has already been removed from the index"
  }
}

pure empty_plan(node_count: Int) -> Plan {
  {
    roots: [],
    targets: [],
    disposition: [Retain for _ in range(node_count)],
    skipped: [],
    apparent: 0,
    allocated: 0,
  }
}

type Work = {disposition: List[Disposition], targets: List[Int], skipped: List[Skipped]}

pure classify_subtree(tree: model.Tree, id: Int, initial: Work) -> Work {
  var work = initial
  let node = tree.nodes[id]
  if node.kind == .Directory and (node.state == .Complete or node.state == .Incomplete) {
    let skipped_start = work.skipped.len()
    var all_removed = true
    var any_planned = false
    var child = node.first_child
    while child != null {
      let at = child ?? 0
      work = classify_subtree(tree, at, work)
      let outcome = work.disposition[at]
      all_removed = all_removed and outcome == Remove
      any_planned = any_planned or outcome != Retain
      child = tree.nodes[at].next_sibling
    }
    var outcome: Disposition = Retain
    if node.state == .Complete and all_removed {
      work.targets += [id]
      outcome = Remove
    } else if any_planned {
      outcome = Enter
    } else {
      work.skipped = work.skipped[..skipped_start]
      work.skipped += [{node: id, reason: skip_reason(node.state)}]
      outcome = Retain
    }
    work.disposition[id] = outcome
    return work
  }
  if node.state == .Complete {
    work.disposition[id] = Remove
    work.targets += [id]
  } else {
    work.skipped += [{node: id, reason: skip_reason(node.state)}]
    work.disposition[id] = Retain
  }
  work
}

pure has_selected_ancestor(tree: model.Tree, selected: Set[Int], id: Int) -> Bool {
  var current = tree.nodes[id].parent
  while current != null {
    let at = current ?? 0
    return true when at in selected
    current = tree.nodes[at].parent
  }
  false
}

pure unique_selection(selected: List[Int]) -> List[Int] {
  var unique: List[Int] = []
  var seen: Set[Int] = set.empty()
  for id in selected {
    if id not in seen {
      unique += [id]
      seen = seen.add(id)
    }
  }
  unique
}

## Plans a browser deletion, entering partially deletable directories.
export pure browser_plan(tree: model.Tree, selected: List[Int]) -> PlanResult {
  var rejected: List[Rejection] = []
  if selected.is_empty() {
    rejected += [{node: null, reason: "no entries are selected"}]
  }
  let unique = unique_selection(selected)
  let chosen: Set[Int] = unique.to_set()
  for id in unique {
    if id < 0 or id >= tree.nodes.len() {
      rejected += [{node: id, reason: "absent from the index"}]
    } else if id == tree.root or tree.nodes[id].kind == .Root {
      rejected += [{node: id, reason: "the scan root is protected"}]
    } else if has_selected_ancestor(tree, chosen, id) {
      rejected += [{node: id, reason: "already covered by a selected parent directory"}]
    }
  }
  if !unique.is_empty() {
    let root_state = tree.nodes[tree.root].state
    if root_state != model.Complete and root_state != model.Incomplete and root_state != model.Pending {
      rejected += [{node: null, reason: "root totals are stale; rescan first"}]
    }
  }
  if !rejected.is_empty() {
    return {ok: false, plan: empty_plan(tree.nodes.len()), rejected: rejected}
  }

  var work: Work = {
    disposition: [Retain for _ in range(tree.nodes.len())],
    targets: [],
    skipped: [],
  }
  var roots: List[Int] = []
  for id in unique {
    work = classify_subtree(tree, id, work)
    if work.disposition[id] != Retain {
      roots += [id]
    }
  }
  if work.targets.is_empty() {
    return {
      ok: false,
      plan: empty_plan(tree.nodes.len()),
      rejected: [{node: entry.node, reason: entry.reason} for entry in work.skipped],
    }
  }

  var apparent = 0
  var allocated = 0
  for target in work.targets {
    let node = tree.nodes[target]
    let covered = if let parent = node.parent {
      work.disposition[parent] == Remove
    } else {
      false
    }
    if !covered {
      apparent += node.apparent
      allocated += node.allocated
    }
  }
  {
    ok: true,
    plan: {
      roots: roots,
      targets: work.targets,
      disposition: work.disposition,
      skipped: work.skipped,
      apparent: apparent,
      allocated: allocated,
    },
    rejected: [],
  }
}

## Removes every planned target in postorder and reports the outcome.
export proc execute(tree: model.Tree, plan: Plan) [fs, error] -> Result[Executed, Error] {
  var current = tree
  var deleted = 0
  var failed = 0
  var messages: List[Str] = []
  for target in plan.targets {
    let node = current.nodes[target]
    let removed = if node.kind == .Directory {
      node.absolute.remove_dir()
    } else {
      node.absolute.unlink()
    }
    match removed {
      Ok(_) => {
        deleted += 1
        current = model.tombstone(current, target)
      }
      Err(failure) => {
        failed += 1
        messages += [f"{node.absolute.display()}: {failure.message}"]
        current = model.mark_stale(current, target)
      }
    }
  }
  Ok({
    tree: current,
    report: {deleted: deleted, failed: failed, skipped: plan.skipped.len(), messages: messages},
  })
}

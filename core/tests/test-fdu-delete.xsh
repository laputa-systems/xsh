use core.lib.fdu_model as model
use core.lib.fdu_delete as delete

type Fixture = {tree: model.Tree, directory: Int, file: Int, second: Int}

pure fixture() -> Fixture {
  var tree = model.empty()
  let root = model.append(tree, null, b"root", p"/root", model.Root, 0, 0, null, 1, model.Pending)
  tree = root.tree
  let directory = model.append(tree, root.id, b"dir", p"/root/dir", model.Directory, 0, 0, null, 2, model.Pending)
  tree = directory.tree
  let file = model.append(tree, directory.id, b"file", p"/root/dir/file", model.File, 10, 512, null, 1, model.Complete)
  tree = file.tree
  tree = model.settle(tree, file.id, model.Complete)
  let second = model.append(tree, directory.id, b"second", p"/root/dir/second", model.File, 0, 0, null, 1, model.Complete)
  tree = second.tree
  tree = model.settle(tree, second.id, model.Complete)
  tree = model.settle(tree, directory.id, model.Complete)
  tree = model.settle(tree, root.id, model.Complete)
  {tree: tree, directory: directory.id, file: file.id, second: second.id}
}

test test_fdu_browser_plan_emits_directory_descendants_before_their_parent {
  let built = fixture()
  let result = delete.browser_plan(built.tree, [built.directory])
  assert result.ok
  assert result.plan.targets == [built.file, built.second, built.directory]
  assert result.plan.skipped.is_empty()
  assert result.plan.apparent == 10
}

test test_fdu_browser_scan_errors_inside_a_directory_skip_only_that_entry {
  let built = fixture()
  var tree = model.set_state(built.tree, built.file, model.Incomplete)
  tree = model.set_state(tree, built.directory, model.Incomplete)
  tree = model.set_state(tree, tree.root, model.Incomplete)
  let result = delete.browser_plan(tree, [built.directory])
  assert result.ok
  assert result.plan.targets == [built.second]
  assert result.plan.roots == [built.directory]
  assert result.plan.disposition[built.directory] == delete.Enter
  assert result.plan.skipped.len() == 1
  assert result.plan.skipped[0].node == built.file
  assert result.plan.skipped[0].reason == "scan is incomplete"
}

test test_fdu_browser_directory_with_nothing_deletable_is_one_skipped_entry {
  let built = fixture()
  var tree = model.set_state(built.tree, built.file, model.Incomplete)
  tree = model.set_state(tree, built.second, model.Incomplete)
  tree = model.set_state(tree, built.directory, model.Incomplete)
  let top = model.append(tree, tree.root, b"top", p"/root/top", model.File, 5, 512, null, 1, model.Complete)
  tree = top.tree
  tree = model.settle(tree, top.id, model.Complete)
  let result = delete.browser_plan(tree, [built.directory, top.id])
  assert result.ok
  assert result.plan.targets == [top.id]
  assert result.plan.roots == [top.id]
  assert result.plan.skipped.len() == 1
  assert result.plan.skipped[0].node == built.directory
}

test test_fdu_browser_selection_with_nothing_deletable_is_rejected_with_each_reason {
  let built = fixture()
  var tree = model.set_state(built.tree, built.file, model.Incomplete)
  tree = model.set_state(tree, built.directory, model.Mount)
  let result = delete.browser_plan(tree, [built.directory])
  assert ! result.ok
  assert result.rejected.len() == 1
  assert result.rejected[0].node == built.directory
  assert result.rejected[0].reason == "mount boundary is protected"
}

test test_fdu_browser_incomplete_root_scan_still_plans_complete_entries {
  let built = fixture()
  let tree = model.set_state(built.tree, built.tree.root, model.Incomplete)
  let result = delete.browser_plan(tree, [built.file])
  assert result.ok
  assert result.plan.targets == [built.file]
}

test test_fdu_browser_running_scan_plans_finished_subtrees_and_skips_directories_still_scanning {
  let built = fixture()
  let scanning = model.append(built.tree, built.tree.root, b"scanning", p"/root/scanning", model.Directory, 0, 0, null, 2, model.Pending)
  var tree = model.set_state(scanning.tree, built.tree.root, model.Pending)
  let result = delete.browser_plan(tree, [built.directory, scanning.id])
  assert result.ok
  assert result.plan.targets == [built.file, built.second, built.directory]
  assert result.plan.disposition[scanning.id] == delete.Retain
  assert result.plan.skipped.len() == 1
  assert result.plan.skipped[0].node == scanning.id
  assert result.plan.skipped[0].reason == "scan is still in progress"
}

test test_fdu_browser_excluded_entries_are_skipped_and_never_entered {
  for reason in [model.Mount, model.Alias] {
    let built = fixture()
    let nested = model.append(built.tree, built.directory, b"nested", p"/root/dir/nested", model.Directory, 0, 0, null, 2, reason)
    let tree = nested.tree
    let result = delete.browser_plan(tree, [built.directory])
    assert result.ok
    assert result.plan.targets == [built.file, built.second]
    assert result.plan.disposition[nested.id] == delete.Retain
    assert result.plan.disposition[built.directory] == delete.Enter
    assert result.plan.skipped.len() == 1
    assert result.plan.skipped[0].node == nested.id
  }
}

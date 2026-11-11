use core.lib.fdu_scan as scan
use core.lib.fdu_model as model

test test_fdu_indexed_scan_reports_tree_entries_and_root_listed { |ctx|
  let temp = test.temp_dir(ctx, name: "fdu-indexed")?
  let root = fp"{temp}/root"
  root.mkdir()
  fp"{root}/sub".mkdir()
  fp"{root}/sub/file.txt".write("sample")
  fp"{root}/top.bin".write("top")
  let tree = scan.tree(root)?
  assert model.children(tree, tree.root).len() == 2
  assert tree.nodes[tree.root].state == model.Complete
  assert tree.nodes[tree.root].descendants == 3
}

test test_fdu_indexed_scan_with_exclusions_omits_excluded_paths { |ctx|
  let temp = test.temp_dir(ctx, name: "fdu-exclusions")?
  let root = fp"{temp}/root"
  root.mkdir()
  fp"{root}/keep".mkdir()
  fp"{root}/omit".mkdir()
  fp"{root}/keep/file.txt".write("kept")
  fp"{root}/omit/file.txt".write("omitted")
  let tree = scan.tree(root, excludes: [p"omit"])?
  let names: List[Str] = [tree.nodes[id].name.utf8() ?? "" for id in model.children(tree, tree.root)]
  assert "keep" in names
  assert "omit" not in names
  assert tree.nodes[tree.root].state == model.Incomplete
}

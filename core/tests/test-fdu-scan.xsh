use core.lib.fdu_scan as scan
use core.lib.fdu_model as model
use core.lib.fdu_ui as ui
use core.lib.tui_browser as browser
use core.lib.tui_colors as colors
use core.lib.tui_surface as surface

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

test test_fdu_displays_incomplete_directory_from_scan_record { |ctx|
  let temp = test.temp_dir(ctx, name: "fdu-incomplete")?
  let root = fp"{temp}/root"
  root.mkdir()
  fp"{root}/sub".mkdir()
  fp"{root}/sub/keep".write("kept")
  fp"{root}/sub/omit".write("omitted")
  let tree = scan.tree(root, excludes: [p"sub/omit"])?
  let rows = ui.visible_rows(tree, tree.root, "", ui.Name, ui.Apparent)
  assert tree.nodes[rows[0]].state == model.Incomplete
  let view: ui.View = {
    tree: tree, current: tree.root, rows: rows,
    browser: browser.reconcile(browser.initial(), rows), filter: "",
    size_mode: ui.Apparent, sort_mode: ui.Name, dialog: ui.None,
    message: null, read_only: false, pending: null, indexed: tree.nodes.len() - 1,
    scan_errors: 0, colors: colors.default_ls_colors(),
  }
  let rendered = ui.render(view, 120, 30, 0)?
  let display = surface.cell_plain(rendered.screen)
  let sub_lines = [line for line in display.split("\n") if "sub/" in line]
  assert sub_lines.len() == 1, display
  assert "!" in sub_lines[0] and "~" in sub_lines[0], sub_lines[0]
}

#!/bin/xsh
use core.lib.tui_browser as browser
use core.lib.tui_colors as colors
use core.lib.tui_input as input
use core.lib.tui_surface
use core.lib.tui_terminal
use core.lib.fdu_delete as fdu_delete
use core.lib.fdu_model as model
use core.lib.fdu_scan as fdu_scan
use core.lib.fdu_ui as ui
use core.lib.monotonic
use lib.gnu

type Options = {
  read_only: Bool,
  apparent: Bool,
  interactive: Bool,
  summary: Bool,
  excludes: List[Path],
  target: Path,
  target_name: Bytes,
}

type App = {
  tree: model.Tree,
  root_path: Path,
  root_name: Bytes,
  excludes: List[Path],
  browser: browser.State,
  current: Int,
  filter: Str,
  size_mode: ui.SizeMode,
  sort_mode: ui.SortMode,
  dialog: ui.Dialog,
  message: Str?,
  read_only: Bool,
  pending: fdu_delete.Plan?,
  colors: colors.LsColors,
  indexed: Int,
  scan_errors: Int,
}

enum Action {
  None, Quit, MoveUp, MoveDown, PageUp, PageDown, Home, End, Enter, Parent,
  Mark, Range, MarkAll, Clear, Split, Sort, Size, FilterStart, Help,
  DeleteStart, Rescan, Switch, Cancel,
}

type Handled = {app: App, quit: Bool}

type Drawn = {app: App, screen: tui_surface.CellScreen}

const USAGE = "Usage: fdu [--apparent] [-x PATH | --exclude PATH]... [--summary | --interactive] [--read-only] [PATH]\nBrowse disk usage in a terminal or write a summary of directory sizes.\n"

proc parse_options(argv: List[Str]) [process, error] -> Result[Options, Error] {
  var read_only = false
  var apparent = false
  var interactive = false
  var summary = false
  var excludes: List[Path] = []
  var target: Path? = null
  var target_name: Bytes = b"."
  var positional_only = false
  var index = 0
  while index < argv.len() {
    let raw = argv[index]
    index += 1
    if ! positional_only {
      if raw == "--" {
        positional_only = true
        continue
      }
      if raw == "-h" or raw == "--help" {
        print $USAGE
        exit 0
      }
      if raw == "-a" or raw == "--apparent" {
        apparent = true
        continue
      }
      if raw == "--read-only" {
        read_only = true
        continue
      }
      if raw == "--summary" {
        summary = true
        continue
      }
      if raw == "--interactive" {
        interactive = true
        continue
      }
      if raw == "-x" or raw == "--exclude" {
        guard index < argv.len() else {
          return Err(error.failure(f"option '{raw}' requires an argument"))
        }
        excludes += [fp"{argv[index]}"]
        index += 1
        continue
      }
      if raw.starts_with("-x") and raw.byte_len() > 2 {
        excludes += [fp"{raw.byte_slice(2)}"]
        continue
      }
      if raw.starts_with("-") and raw.byte_len() > 1 {
        return Err(error.failure(f"unrecognized option '{raw}'"))
      }
    }
    guard target == null else {
      return Err(error.failure(f"unexpected argument '{raw}'"))
    }
    target = fp"{raw}"
    target_name = bytes.from_text(raw)
  }
  {
    read_only: read_only,
    apparent: apparent,
    interactive: interactive,
    summary: summary,
    excludes: excludes,
    target: target ?? p".",
    target_name: target_name,
  }
}

pure rows_of(app: App) -> List[Int] {
  ui.visible_rows(app.tree, app.current, app.filter, app.sort_mode, app.size_mode)
}

pure with_marks(state: browser.State, marks: List[Int]) -> browser.State {
  var marked_set: Set[Int] = set.empty()
  for id in marks {
    marked_set = marked_set.add(id)
  }
  let cursor = if marks.is_empty() {
    null
  } else {
    let current = state.marked_cursor ?? 0
    if current < 0 {
      0
    } else if current >= marks.len() {
      marks.len() - 1
    } else {
      current
    }
  }
  {...state, marks: marks, marked_set: marked_set, marked_cursor: cursor}
}

pure to_view(app: App) -> ui.View {
  {
    tree: app.tree,
    current: app.current,
    rows: rows_of(app),
    browser: app.browser,
    filter: app.filter,
    size_mode: app.size_mode,
    sort_mode: app.sort_mode,
    dialog: app.dialog,
    message: app.message,
    read_only: app.read_only,
    pending: app.pending,
    indexed: app.indexed,
    scan_errors: app.scan_errors,
    colors: app.colors,
  }
}

proc initial_app(tree: model.Tree, root_path: Path, root_name: Bytes, excludes: List[Path], read_only: Bool, apparent: Bool) [env] -> App {
  let mode = if apparent { ui.Apparent } else { ui.Allocated }
  let rows = ui.visible_rows(tree, tree.root, "", ui.Size, mode)
  let state = browser.reconcile(browser.initial(), rows)
  {
    tree: tree,
    root_path: root_path,
    root_name: root_name,
    excludes: excludes,
    browser: state,
    current: tree.root,
    filter: "",
    size_mode: mode,
    sort_mode: ui.Size,
    dialog: ui.None,
    message: null,
    read_only: read_only,
    pending: null,
    colors: colors.parse_ls_colors(env.get_or("LS_COLORS", "") ?? ""),
    indexed: tree.nodes.len() - 1,
    scan_errors: 0,
  }
}

pure refresh_rows(app: App, columns: Int, rows: Int) -> App {
  let listing = rows_of(app)
  var next = app
  next.browser = browser.reconcile(app.browser, listing)
  next.browser = browser.normalize(next.browser, columns, rows)
  next
}

pure toggle_mark(app: App, columns: Int, rows: Int) -> App {
  let selected = app.browser.cursor
  if selected == null {
    return app
  }
  let cursor = selected ?? 0
  var next = app
  if cursor in app.browser.marked_set {
    next.browser = with_marks(app.browser, [id for id in app.browser.marks if id != cursor])
  } else if let ancestor = model.marked_ancestor(app.tree, cursor, app.browser.marked_set) {
    next.message = f"Already covered by marked directory {model.relative_display(app.tree, ancestor)}."
    return next
  } else {
    let kept = [id for id in app.browser.marks if ! model.is_inside(app.tree, id, cursor)]
    next.browser = with_marks(app.browser, kept + [cursor])
  }
  next.message = null
  next.browser = browser.move_cursor(next.browser, rows_of(next), 1)
  next.browser = browser.normalize(next.browser, columns, rows)
  next
}

pure switch_pane(app: App, columns: Int, rows: Int) -> App {
  if app.browser.marks.is_empty() {
    return {...app, message: "Nothing is marked."}
  }
  let focus = match app.browser.focus {
    browser.Browser => browser.Marked
    browser.Marked => browser.Browser
  }
  let next = {...app, browser: {...app.browser, focus: focus}}
  {...next, browser: browser.normalize(next.browser, columns, rows)}
}

pure cycle_sort(app: App, columns: Int, rows: Int) -> App {
  let sort = match app.sort_mode {
    ui.Size => ui.Name
    ui.Name => ui.Date
    ui.Date => ui.Size
  }
  let name = match sort {
    ui.Size => "size"
    ui.Name => "name"
    ui.Date => "date"
  }
  var next: App = {...app, sort_mode: sort, message: f"Sorted by {name}"}
  next = refresh_rows(next, columns, rows)
  next
}

pure toggle_size(app: App, columns: Int, rows: Int) -> App {
  let mode = match app.size_mode {
    ui.Allocated => ui.Apparent
    ui.Apparent => ui.Allocated
  }
  refresh_rows({...app, size_mode: mode}, columns, rows)
}

pure change_directory(app: App, directory: Int, columns: Int, rows: Int) -> App {
  var next: App = {...app, current: directory, filter: app.filter, pending: null}
  next.browser = {...app.browser, range_mode: false, range_anchor: null}
  if let marked = model.marked_ancestor(app.tree, directory, app.browser.marked_set) {
    next.message = f"Inside marked directory {model.relative_display(app.tree, marked)}."
  } else if directory in app.browser.marked_set {
    next.message = f"Inside marked directory {model.relative_display(app.tree, directory)}."
  } else {
    next.message = null
  }
  next = refresh_rows(next, columns, rows)
  next
}

pure enter_cursor(app: App, columns: Int, rows: Int) -> App {
  guard let cursor = app.browser.cursor else {
    return app
  }
  let node = app.tree.nodes[cursor]
  if node.kind != .Directory {
    return {...app, message: "This entry is not a directory."}
  }
  match node.state {
    model.Mount => {...app, message: "This directory is an excluded mount boundary."}
    model.Alias => {...app, message: "This directory is an unsupported filesystem alias."}
    else => change_directory(app, cursor, columns, rows)
  }
}

pure go_parent(app: App, columns: Int, rows: Int) -> App {
  guard let parent = app.tree.nodes[app.current].parent else {
    return app
  }
  change_directory(app, parent, columns, rows)
}

pure begin_delete(app: App, columns: Int, rows: Int) -> App {
  if app.read_only {
    return {...app, message: "This session is read only; deletion is disabled."}
  }
  if app.browser.focus != browser.Marked {
    return {...app, message: "Press Tab to review marks, then Ctrl-R."}
  }
  if app.browser.marks.is_empty() {
    return {...app, message: "Nothing is marked."}
  }
  let result = fdu_delete.browser_plan(app.tree, app.browser.marks)
  if ! result.ok {
    let first = result.rejected[0]
    let more = result.rejected.len() - 1
    let detail = if first.node != null {
      f"{model.relative_display(app.tree, first.node ?? 0)}: {first.reason}"
    } else {
      first.reason
    }
    let suffix = if more > 0 { f" (+{more} more)" } else { "" }
    return {...app, message: f"Nothing can be deleted: {detail}{suffix}"}
  }
  {...app, pending: result.plan, message: null, dialog: ui.None}
}

proc execute_delete(app: App, columns: Int, rows: Int) [fs, error] -> App {
  guard let plan = app.pending else {
    return app
  }
  let executed = fdu_delete.execute(app.tree, plan)?
  let report = executed.report
  var parts = [f"Deleted {report.deleted}"]
  if report.skipped > 0 {
    parts += [f"{report.skipped} skipped"]
  }
  if report.failed > 0 {
    parts += [f"{report.failed} failed"]
  }
  let survivors = [id for id in app.browser.marks if keeps_mark(executed.tree, id)]
  var next: App = {
    ...app,
    tree: executed.tree,
    pending: null,
    message: parts.join(" · "),
    indexed: live_entries(executed.tree),
    browser: with_marks(app.browser, survivors),
  }
  next.browser = browser.normalize(next.browser, columns, rows)
  next = refresh_rows(next, columns, rows)
  next
}

pure live_entries(tree: model.Tree) -> Int {
  var count = 0
  for index in range(tree.nodes.len()) {
    if index != tree.root and tree.nodes[index].state != model.Tombstone {
      count += 1
    }
  }
  count
}

pure keeps_mark(tree: model.Tree, id: Int) -> Bool {
  if tree.nodes[id].state == model.Tombstone {
    return false
  }
  if tree.nodes[id].state == model.Stale {
    return true
  }
  var pending = [id]
  while ! pending.is_empty() {
    let at = pending[pending.len() - 1]
    pending = pending[..pending.len() - 1]
    if tree.nodes[at].state == model.Stale {
      return true
    }
    for child in model.children(tree, at) {
      pending += [child]
    }
  }
  false
}

proc rescan(app: App, columns: Int, rows: Int) [fs, error] -> App {
  let tree = fdu_scan.tree(app.root_path, excludes: app.excludes, name: app.root_name)?
  let rows_next = ui.visible_rows(tree, tree.root, app.filter, app.sort_mode, app.size_mode)
  let state = browser.reconcile(browser.initial(), rows_next)
  refresh_rows({
    ...app,
    tree: tree,
    current: tree.root,
    browser: state,
    pending: null,
    message: "Rescanned.",
    indexed: tree.nodes.len() - 1,
  }, columns, rows)
}

pure action_for(key: input.Key) -> Action {
  match key {
    input.Up | input.Text("k") => MoveUp
    input.Down | input.Text("j") => MoveDown
    input.PageUp => PageUp
    input.PageDown => PageDown
    input.Home => Home
    input.End => End
    input.Enter | input.Right | input.Text("l") => Enter
    input.Left | input.Backspace | input.Text("h") => Parent
    input.Tab => Switch
    input.Text("d") => Mark
    input.Text("v") => Range
    input.Text("*") => MarkAll
    input.Text("c") => Clear
    input.Text("-") => Split
    input.Text("s") => Sort
    input.Text("a") => Size
    input.Text("/") => FilterStart
    input.Text("?") => Help
    input.Control(18) => DeleteStart
    input.Text("r") => Rescan
    input.Text("q") | input.Cancel => Quit
    input.Escape => Cancel
    else => None
  }
}

proc apply_action(app: App, action: Action, columns: Int, rows: Int) [fs, error] -> Handled {
  if app.pending != null {
    if action == Enter {
      return {app: execute_delete(app, columns, rows), quit: false}
    }
    if action == Cancel {
      return {app: {...app, pending: null}, quit: false}
    }
    return {app: app, quit: false}
  }
  match app.dialog {
    ui.Help(_) => {
      if action == Cancel or action == Help {
        return {app: {...app, dialog: ui.None}, quit: false}
      }
      return {app: app, quit: false}
    }
    ui.Filter(_) => return {app: app, quit: false}
    ui.None => {}
  }
  var next = app
  match action {
    Quit => return {app: app, quit: true}
    None => {}
    Cancel => next.message = null
    Help => next.dialog = ui.Help(app.browser.focus)
    FilterStart => if app.browser.focus == browser.Browser {
      next.dialog = ui.Filter(app.filter)
      next.message = null
    } else {
      next.message = "That applies to the list; press Tab."
    }
    Switch => next = switch_pane(app, columns, rows)
    Split => {
      next.browser = {...app.browser, split: browser.split_toggled(app.browser.split)}
    }
    Sort => next = cycle_sort(app, columns, rows)
    Size => next = toggle_size(app, columns, rows)
    Rescan => next = rescan(app, columns, rows)
    DeleteStart => next = begin_delete(app, columns, rows)
    Enter => next = if app.browser.focus == browser.Marked {
      reveal_marked(app, columns, rows)
    } else {
      enter_cursor(app, columns, rows)
    }
    Parent => next = go_parent(app, columns, rows)
    Mark => next = if app.browser.focus == browser.Marked {
      unmark_at_cursor(app, columns, rows)
    } else {
      toggle_mark(app, columns, rows)
    }
    Clear => {
      next.browser = with_marks(app.browser, [])
      next.message = null
      next.browser = browser.normalize(next.browser, columns, rows)
    }
    Range => if app.browser.focus == browser.Marked {
      next.message = "That applies to the list; press Tab."
    } else {
      next.browser = browser.toggle_range(app.browser)
    }
    MarkAll => if app.browser.focus == browser.Marked {
      next.message = "That applies to the list; press Tab."
    } else {
      next.browser = browser.mark_all(app.browser, rows_of(app))
    }
    MoveUp => next.browser = browser.move_cursor(app.browser, rows_of(app), -1)
    MoveDown => next.browser = browser.move_cursor(app.browser, rows_of(app), 1)
    PageUp => next.browser = browser.move_cursor(app.browser, rows_of(app), -15)
    PageDown => next.browser = browser.move_cursor(app.browser, rows_of(app), 15)
    Home => next.browser = browser.move_to_edge(app.browser, rows_of(app), false)
    End => next.browser = browser.move_to_edge(app.browser, rows_of(app), true)
  }
  next.browser = browser.normalize(next.browser, columns, rows)
  {app: next, quit: false}
}

pure unmark_at_cursor(app: App, columns: Int, rows: Int) -> App {
  let at = app.browser.marked_cursor ?? 0
  if at < 0 or at >= app.browser.marks.len() {
    return app
  }
  let marks = [app.browser.marks[index] for index in range(app.browser.marks.len()) if index != at]
  var next = {...app, browser: with_marks(app.browser, marks)}
  next.browser = browser.normalize(next.browser, columns, rows)
  next
}

pure reveal_marked(app: App, columns: Int, rows: Int) -> App {
  let at = app.browser.marked_cursor ?? 0
  if at < 0 or at >= app.browser.marks.len() {
    return app
  }
  let id = app.browser.marks[at]
  guard let parent = app.tree.nodes[id].parent else {
    return app
  }
  let had_filter = app.filter != ""
  var next = change_directory(app, parent, columns, rows)
  if had_filter {
    next.message = "Filter cleared to show the marked entry."
  }
  next.filter = ""
  next = refresh_rows(next, columns, rows)
  if id in rows_of(next) {
    next.browser = browser.select(next.browser, rows_of(next), id)
  }
  next.browser = {...next.browser, focus: browser.Browser}
  next.browser = browser.normalize(next.browser, columns, rows)
  next
}

pure apply_filter(app: App, value: Str, columns: Int, rows: Int) -> App {
  var next: App = {...app, filter: value, dialog: ui.None, message: null}
  next.browser = {...app.browser, range_mode: false, range_anchor: null}
  refresh_rows(next, columns, rows)
}

proc apply_key(app: App, key: input.Key, columns: Int, rows: Int) [fs, error] -> Handled {
  match app.dialog {
    ui.Filter(value) => {
      match key {
        input.Text(text) => {app: {...app, dialog: ui.Filter(value + text)}, quit: false}
        input.Backspace => {
          let characters = value.count_chars()
          let shorter = if characters == 0 { "" } else { value.split("") |> take(characters - 1).join("") }
          {app: {...app, dialog: ui.Filter(shorter)}, quit: false}
        }
        input.Enter => {app: apply_filter(app, value, columns, rows), quit: false}
        input.Escape => {app: {...app, dialog: ui.None}, quit: false}
        input.Cancel => {app: app, quit: true}
        else => {app: app, quit: false}
      }
    }
    ui.Help(_) => {
      if key == input.Escape or key == input.Text("?") {
        {app: {...app, dialog: ui.None}, quit: false}
      } else if key == input.Cancel {
        {app: app, quit: true}
      } else {
        {app: app, quit: false}
      }
    }
    ui.None => apply_action(app, action_for(key), columns, rows)
  }
}

proc draw(app: App, previous: tui_surface.CellScreen, columns: Int, rows: Int, now: Int) [io, error] -> Result[Drawn, Error] {
  let rendered = ui.render(to_view(app), columns, rows, now)?
  let patch_bytes = tui_surface.cell_diff(previous, rendered.screen)
  if patch_bytes != "" {
    tui_terminal.present(patch_bytes)
  }
  Ok({
    app: {...app, browser: rendered.browser},
    screen: rendered.screen,
  })
}

proc interactive(app: App) [env, fs, process, io, time, error] -> Result[Unit, Error] {
  let session = tui_terminal.enter()?
  var current = app
  var size = unix.window_size(1)?
  var columns = size.cols
  var rows = size.rows
  var previous = tui_surface.cell_screen(0, 0)?
  var decoder = input.initial()
  var dirty = true
  var quit = false
  defer tui_terminal.leave(session)
  while ! quit {
    let now_ms = monotonic.now()?.ms
    let timed = input.timeout(decoder, now_ms)
    decoder = timed.state
    for event in timed.events {
      match event {
        input.Pressed(key) => {
          let handled = apply_key(current, key, columns, rows)
          current = handled.app
          dirty = true
          if handled.quit {
            quit = true
          }
        }
        input.Paste(_) => {}
        input.Eof => quit = true
      }
      if quit {
        break
      }
    }
    if quit {
      break
    }
    let measured = unix.window_size(1)?
    if measured.cols != columns or measured.rows != rows {
      columns = measured.cols
      rows = measured.rows
      dirty = true
    }
    if dirty {
      let drawn = draw(current, previous, columns, rows, time.now() / 1000)?
      current = drawn.app
      previous = drawn.screen
      dirty = false
    }
    let wait_ms = input.wait_ms(decoder, now_ms, 50)
    let ready = unix.poll_fd(0, ["readable"], timeout_ms: wait_ms)?
    if "readable" in ready {
      let chunk = unix.read_fd(0, 1)?
      let decoded = if chunk.is_empty() {
        input.finish(decoder)
      } else {
        input.push(decoder, chunk, monotonic.now()?.ms)
      }
      decoder = decoded.state
      for event in decoded.events {
        match event {
          input.Pressed(key) => {
            let handled = apply_key(current, key, columns, rows)
            current = handled.app
            dirty = true
            if handled.quit {
              quit = true
            }
          }
          input.Paste(_) => {}
          input.Eof => quit = true
        }
        if quit {
          break
        }
      }
    }
  }
  let pending = input.unconsumed(decoder)
  let _ = tui_terminal.hand_off(pending)
  Ok()
}

proc main(...argv: List[Str]) [env, fs, process, io, time, error] {
  let options = parse_options(argv) ?? { |failure|
    gnu.error(failure.message)
    eprint $USAGE
    exit 2
  }
  let tree = fdu_scan.tree(options.target, excludes: options.excludes, name: options.target_name) ?? { |failure|
    gnu.error(failure.message)
    exit 1
  }
  if options.summary {
    let root = tree.nodes[tree.root]
    let size = if options.apparent { root.apparent } else { root.allocated }
    print f"{tui_surface.format_size(size)}\t{options.target.display()}"
    return
  }
  if ! unix.isatty(0) or ! unix.isatty(1) {
    gnu.error("--interactive requires terminal input and output")
    exit 1
  }
  let app = initial_app(tree, options.target, options.target_name, options.excludes, options.read_only, options.apparent)
  interactive(app)?
}

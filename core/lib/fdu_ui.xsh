##! Renders fdu's browser, marked pane, status strip, and modals.
##!
##! The layout follows fdu: a reversed title strip, two panes that are side by
##! side or stacked, a status strip that leads with the entry count, and a
##! centered modal for help and filtering.
use tui_browser as browser
use tui_colors as colors
use tui_surface
use tui
use core.lib.fdu_model as model
use core.lib.fdu_delete as delete

## Selects which size a row reports.
export enum SizeMode { Allocated, Apparent }

## Selects how a directory's rows are ordered.
export enum SortMode { Size, Name, Date }

## Names the open modal, if any.
export enum Dialog { None, Help(browser.Pane), Filter(Str) }

## Reports the focused pane, split, and marks of the browser.
export type View = {
  tree: model.Tree,
  current: Int,
  rows: List[Int],
  browser: browser.State,
  filter: Str,
  size_mode: SizeMode,
  sort_mode: SortMode,
  dialog: Dialog,
  message: Str?,
  read_only: Bool,
  pending: delete.Plan?,
  indexed: Int,
  scan_errors: Int,
  colors: colors.LsColors,
}

## Returns the rendered screen with the scroll positions the frame implies.
export type Rendered = {screen: tui_surface.CellScreen, browser: browser.State}

type PaneFrame = {screen: tui_surface.CellScreen, top: Int}

const BAR_CELLS_WIDE = 10
const BAR_CELLS_NARROW = 5
const SIZE_COLUMN_WIDTH = 11
const AGE_COLUMN_WIDTH = 5
const MIN_MESSAGE_WIDTH = 12

## Returns the selected size of one node.
export pure size_of(node: model.Node, mode: SizeMode) -> Int {
  match mode {
    Allocated => node.allocated
    Apparent => node.apparent
  }
}

## Returns the node name as lossy text for filtering.
export pure name_text(node: model.Node) -> Str {
  node.name.utf8() ?? ""
}

## Returns one directory's rows in the selected order.
export pure list_rows(tree: model.Tree, directory: Int, sort: SortMode, size: SizeMode) -> List[Int] {
  let children = model.children(tree, directory)
  match sort {
    Name => children |> sort-by { |id| name_text(tree.nodes[id]) }
    Date => {
      let ordered = children |> sort-by { |id| name_text(tree.nodes[id]) }
      ordered |> sort-by { |id| -(tree.nodes[id].modified ?? -9223372036854775807) }
    }
    Size => {
      let ordered = children |> sort-by { |id| name_text(tree.nodes[id]) }
      ordered |> sort-by { |id| -size_of(tree.nodes[id], size) }
    }
  }
}

## Reports whether a row's name contains the filter case-insensitively.
export pure matches_filter(node: model.Node, filter: Str) -> Bool {
  return true when filter == ""
  name_text(node).lower().find(filter.lower()) != null
}

## Returns the rows of one directory that pass the filter.
export pure visible_rows(tree: model.Tree, directory: Int, filter: Str, sort: SortMode, size: SizeMode) -> List[Int] {
  let rows = list_rows(tree, directory, sort, size)
  if filter == "" {
    return rows
  }
  [id for id in rows if matches_filter(tree.nodes[id], filter)]
}

## Returns the marked directory that already covers `id`, if any.
export pure covering_mark(view: View) -> Int? {
  if view.current in view.browser.marked_set {
    return view.current
  }
  model.marked_ancestor(view.tree, view.current, view.browser.marked_set)
}

pure percent_column(node: model.Node, mode: SizeMode, total: Int) -> Str {
  if node.state == model.Mount or node.state == model.Alias {
    return "       "
  }
  let value = size_of(node, mode)
  if total == 0 {
    return "  0.0% "
  }
  let tenths = (value * 1000 + total / 2) / total
  let whole = tenths / 10
  let fraction = tenths % 10
  let number = f"{whole}.{fraction}"
  let padding = if number.byte_len() < 5 { tui_surface.repeated(" ", 5 - number.byte_len()) } else { "" }
  f"{padding}{number}% "
}

pure bar_column(node: model.Node, mode: SizeMode, total: Int, cells: Int) -> Str {
  return "" when cells == 0
  let value = size_of(node, mode)
  var filled = 0
  if node.state != model.Mount and node.state != model.Alias and total != 0 and value != 0 {
    let rounded = (value * cells + total / 2) / total
    filled = if rounded < 1 { 1 } else if rounded > cells { cells } else { rounded }
  }
  tui_surface.repeated("█", filled) + tui_surface.repeated(" ", cells - filled) + " "
}

pure state_marker(state: model.State) -> Str {
  match state {
    model.Pending => "…"
    model.Incomplete => "!"
    model.Mount => "M"
    model.Alias => "A"
    model.Stale => "~"
    model.Complete => " "
    model.Tombstone => " "
  }
}

pure state_size(node: model.Node, mode: SizeMode) -> Str {
  match node.state {
    model.Mount => "mount"
    model.Alias => "alias"
    model.Incomplete | model.Stale => f"~{tui_surface.format_size(size_of(node, mode))}"
    else => tui_surface.format_size(size_of(node, mode))
  }
}

pure kind_suffix(node: model.Node) -> Str {
  match node.kind {
    model.Directory | model.Root => "/"
    model.Symlink => "@"
    else => ""
  }
}

pure entry_kind(node: model.Node) -> colors.EntryKind {
  match node.kind {
    model.Directory | model.Root => colors.Directory
    model.Symlink => colors.Symlink
    model.File => colors.Regular
    model.Other => colors.Other
  }
}

## Returns a row's base style from marks, states, and LS_COLORS.
export pure row_style(view: View, id: Int) -> colors.Style {
  let node = view.tree.nodes[id]
  if id in view.browser.marked_set {
    return colors.RED
  }
  match node.state {
    model.Incomplete | model.Stale => colors.YELLOW
    model.Mount | model.Alias => colors.RED
    else => colors.style_for(view.colors, node.name, entry_kind(node))
  }
}

pure padded(value: Str, width: Int) -> Str {
  tui_surface.pad_to(value, width)
}

pure right(value: Str, width: Int) -> Str {
  let used = tui.cell_width(value)
  if used >= width {
    return value
  }
  tui_surface.repeated(" ", width - used) + value
}

pure apply_rect(screen: tui_surface.CellScreen, area: tui_surface.Rect, text: Str, style: colors.Style) -> tui_surface.CellScreen {
  tui_surface.cell_text(screen, area, text, style)
}

pure border_style(focused: Bool) -> colors.Style {
  if focused { colors.PLAIN } else { colors.DIM }
}

pure title_style(focused: Bool) -> colors.Style {
  if focused { colors.BOLD } else { colors.DIM }
}

pure key_hints(view: View) -> Str {
  match view.browser.focus {
    browser.Browser => if view.browser.marks.is_empty() {
      "d mark · s sort · / filter · ? help · q quit"
    } else {
      "d mark · Tab marked · - split · s sort · / filter · ? help · q quit"
    }
    browser.Marked => if view.read_only {
      "d unmark · Enter show · c clear · Tab list · - split · ? help · q quit"
    } else {
      "d unmark · Enter show · ^R delete · c clear · Tab list · - split · ? help · q quit"
    }
  }
}

pure status_text(view: View, width: Int) -> Str {
  let count = f"{view.indexed} entries" + (match view.scan_errors {
    0 => ""
    1 => " · 1 scan error"
    else => f" · {view.scan_errors} scan errors"
  })
  if let plan = view.pending {
    let skipped = if plan.skipped.is_empty() { "" } else { f" · {plan.skipped.len()} skipped" }
    let plural = if plan.roots.len() == 1 { "" } else { "s" }
    return f" Permanently delete {plan.roots.len()} item{plural} ({plan.targets.len()} entries, {tui_surface.format_size(plan.allocated)})? Enter confirms · Esc cancels{skipped} · {count}"
  }
  if let message = view.message {
    let head = f" {count}"
    let full = f" · {key_hints(view)}"
    let compact = " · ? help · q quit"
    let fits = tui.cell_width(head) + 3 + tui.cell_width(message) + tui.cell_width(full) <= width
    let tail = if fits { full } else { compact }
    let used = tui.cell_width(head) + tui.cell_width(tail) + 3
    let budget = if width > used { width - used } else { 0 }
    let shown = if budget >= MIN_MESSAGE_WIDTH { tui_surface.clip_end(message, budget) } else { message }
    return f"{head} · {shown}{tail}"
  }
  f" {count} · {key_hints(view)}"
}

## Draws the title strip, panes, status strip, and modal.
export proc render(view: View, columns: Int, rows: Int, now: Int) -> Result[Rendered, Error] {
  var screen = tui_surface.cell_screen(columns, rows)?
  let title_height = if rows >= 8 { 1 } else { 0 }
  let status_height = if rows >= 3 { 1 } else { 0 }
  let pane_height = rows - title_height - status_height
  if title_height > 0 {
    screen = apply_rect(
      screen,
      {x: 0, y: 0, width: columns, height: 1},
      padded(title_text(view, columns), columns),
      colors.REVERSED,
    )
  }
  let pane_area: tui_surface.Rect = {x: 0, y: title_height, width: columns, height: pane_height}
  let normalized = browser.normalize(view.browser, columns, rows)
  let side_visible = normalized.side_visible
  var top = normalized.scroll
  var marked_top = normalized.marked_scroll
  if !side_visible {
    let drawn = render_browser(screen, pane_area, {...view, browser: normalized}, top, now)
    screen = drawn.screen
    top = drawn.top
  } else if browser.fits_two_panes(normalized.split, columns, rows) {
    let parts = match normalized.split {
      browser.Vertical => tui_surface.split_columns(pane_area, 55)
      browser.Horizontal => tui_surface.split_rows(pane_area, 60)
    }
    let list = render_browser(screen, parts[0], {...view, browser: normalized}, top, now)
    screen = list.screen
    top = list.top
    let marked = render_marked(screen, parts[1], {...view, browser: normalized}, marked_top, now)
    screen = marked.screen
    marked_top = marked.top
  } else if normalized.focus == browser.Marked {
    let marked = render_marked(screen, pane_area, {...view, browser: normalized}, marked_top, now)
    screen = marked.screen
    marked_top = marked.top
  } else {
    let drawn = render_browser(screen, pane_area, {...view, browser: normalized}, top, now)
    screen = drawn.screen
    top = drawn.top
  }
  if status_height > 0 {
    screen = apply_rect(
      screen,
      {x: 0, y: rows - 1, width: columns, height: 1},
      tui_surface.clip_end(status_text(view, columns), columns),
      if view.pending != null { colors.RED } else { colors.PLAIN },
    )
  }
  match view.dialog {
    None => {}
    Help(focus) => {
      screen = tui_surface.cell_modal(screen, "Help", help_lines(focus))?
    }
    Filter(value) => {
      screen = tui_surface.cell_modal(
        screen,
        "Filter current directory",
        [
          "Type a name fragment; Enter applies it, Esc cancels.",
          f"/ {value}",
        ],
      )?
    }
  }
  Ok({
    screen: screen,
    browser: {...normalized, scroll: top, marked_scroll: marked_top},
  })
}

pure percent_text(value: Int, total: Int) -> Str {
  if total == 0 {
    return "0.0"
  }
  let tenths = (value * 1000 + total / 2) / total
  f"{tenths / 10}.{tenths % 10}"
}

pure title_text(view: View, columns: Int) -> Str {
  var text = " fdu"
  if view.read_only {
    text += " · read only"
  }
  if view.size_mode == .Apparent {
    text += " · apparent sizes"
  }
  if view.filter != "" {
    text += f" · filter {quoted(view.filter)}"
  }
  tui_surface.clip_end(text, columns)
}

pure quoted(text: Str) -> Str {
  "\"" + text + "\""
}

pure render_browser(screen: tui_surface.CellScreen, area: tui_surface.Rect, view: View, initial_top: Int, now: Int) -> PaneFrame {
  let focused = view.browser.focus == browser.Browser
  let node = view.tree.nodes[view.current]
  let total = size_of(node, view.size_mode)
  let relative = model.relative_display(view.tree, view.current)
  let location = if relative == "" {
    tui_surface.escape_name(view.tree.nodes[view.tree.root].name)
  } else {
    f"{tui_surface.escape_name(view.tree.nodes[view.tree.root].name)}/{relative}"
  }
  let counts = f" ({view.rows.len()} shown, {node.descendants} total, {tui_surface.format_size(total)})"
  let inner_width = if area.width > 2 { area.width - 2 } else { 0 }
  let budget = if inner_width > tui.cell_width(counts) + 1 { inner_width - tui.cell_width(counts) - 1 } else { 0 }
  let title = f" {tui_surface.clip_start(location, budget)}{counts}"
  let bordered = tui_surface.cell_border(screen, area, title, border_style(focused), title_style(focused))
  let cursor_index = browser.cursor_index(view.browser, view.rows)
  let window = browser.window_start(initial_top, cursor_index, view.rows.len(), bordered.inner.height)
  let drawn = render_rows(bordered.screen, bordered.inner, view, total, window.start, focused, now)
  {screen: drawn.screen, top: window.start}
}

pure render_rows(
  screen: tui_surface.CellScreen,
  area: tui_surface.Rect,
  view: View,
  directory_total: Int,
  start: Int,
  focused: Bool,
  now: Int,
) -> PaneFrame {
  var current = screen
  if area.height == 0 {
    return {screen: current, top: start}
  }
  let width = area.width
  let show_percent = width >= 52
  let bar_cells = if width >= 63 { BAR_CELLS_WIDE } else if width >= 58 { BAR_CELLS_NARROW } else { 0 }
  let cursor_index = browser.cursor_index(view.browser, view.rows)
  let covered = covering_mark(view)
  for offset in range(area.height) {
    let index = start + offset
    if index >= view.rows.len() {
      break
    }
    let id = view.rows[index]
    let node = view.tree.nodes[id]
    let mark = if id in view.browser.marked_set { "[x]" } else if covered != null { "[=]" } else { "   " }
    let marker = state_marker(node.state)
    let size_text = state_size(node, view.size_mode)
    let percent = if show_percent { percent_column(node, view.size_mode, directory_total) } else { "" }
    let bar = bar_column(node, view.size_mode, directory_total, bar_cells)
    let link = if node.nlink > 1 and node.kind != model.Directory and node.kind != model.Root { "*" } else { " " }
    let age = tui_surface.format_age(node.modified, now)
    let suffix = kind_suffix(node)
    let prefix = f"{mark} {marker}{right(size_text, SIZE_COLUMN_WIDTH)} {percent}{bar}{link}{right(age, AGE_COLUMN_WIDTH)} "
    let name = tui_surface.escape_name(node.name)
    let used = tui.cell_width(prefix) + tui.cell_width(suffix)
    let name_budget = if width > used { width - used } else { 0 }
    let text = padded(f"{prefix}{tui_surface.clip_end(name, name_budget)}{suffix}", width)
    let base = row_style(view, id)
    let style = if cursor_index == index { colors.with_cursor(base, focused) } else { base }
    current = apply_rect(current, {x: area.x, y: area.y + offset, width: area.width, height: 1}, text, style)
  }
  {screen: current, top: start}
}

pure render_marked(screen: tui_surface.CellScreen, area: tui_surface.Rect, view: View, initial_top: Int, now: Int) -> PaneFrame {
  let focused = view.browser.focus == browser.Marked
  var total = 0
  for id in view.browser.marks {
    total += size_of(view.tree.nodes[id], view.size_mode)
  }
  let root_total = size_of(view.tree.nodes[view.tree.root], view.size_mode)
  let percent = percent_text(total, root_total)
  let plural = if view.browser.marks.len() == 1 { "" } else { "s" }
  let title = f" Marked {view.browser.marks.len()} item{plural} ({tui_surface.format_size(total)}, {percent}% of {tui_surface.format_size(root_total)})"
  let bordered = tui_surface.cell_border(
    screen,
    area,
    if area.width > 2 { tui_surface.clip_end(title, area.width - 2) } else { title },
    border_style(focused),
    title_style(focused),
  )
  if view.browser.marks.is_empty() {
    let lines = ["Nothing is marked.", "d marks the selected entry."]
    var current = bordered.screen
    for index in range(lines.len()) {
      if index >= bordered.inner.height {
        break
      }
      current = apply_rect(
        current,
        {x: bordered.inner.x, y: bordered.inner.y + index, width: bordered.inner.width, height: 1},
        tui_surface.fit(lines[index], bordered.inner.width),
        colors.PLAIN,
      )
    }
    return {screen: current, top: initial_top}
  }
  let window = browser.window_start(initial_top, view.browser.marked_cursor, view.browser.marks.len(), bordered.inner.height)
  var current = bordered.screen
  let width = bordered.inner.width
  for offset in range(bordered.inner.height) {
    let index = window.start + offset
    if index >= view.browser.marks.len() {
      break
    }
    let id = view.browser.marks[index]
    let node = view.tree.nodes[id]
    let marker = state_marker(node.state)
    let size_text = state_size(node, view.size_mode)
    let age = tui_surface.format_age(node.modified, now)
    let suffix = kind_suffix(node)
    let prefix = f" {marker}{right(size_text, SIZE_COLUMN_WIDTH)} {right(age, AGE_COLUMN_WIDTH)}  "
    let entry_path = model.relative_display(view.tree, id)
    let used = tui.cell_width(prefix) + tui.cell_width(suffix)
    let budget = if width > used { width - used } else { 0 }
    let text = padded(f"{prefix}{tui_surface.clip_start(entry_path, budget)}{suffix}", width)
    let style = if view.browser.marked_cursor == index {
      colors.with_cursor(colors.RED, focused)
    } else {
      colors.RED
    }
    current = apply_rect(
      current,
      {x: bordered.inner.x, y: bordered.inner.y + offset, width: width, height: 1},
      text,
      style,
    )
  }
  {screen: current, top: window.start}
}

pure help_lines(focus: browser.Pane) -> List[Str] {
  let focus_line = match focus {
    browser.Browser => "Focus: list"
    browser.Marked => "Focus: marked items"
  }
  [
    focus_line,
    "The marked-items pane opens when something is marked. Tab switches",
    "  panes. - stacks the panes instead of placing them side by side.",
    "  When the terminal is too small for both, only the focused one shows.",
    "",
    "List: j/k or arrows move, Enter/l opens, h/Backspace goes up",
    "  (never above the root). d marks, v marks a range, * marks",
    "  matches, c clears marks, / filters, s sorts by size, name, or modified date,",
    "  a switches allocated/apparent sizes, r rescans (clears marks).",
    "Marked items: arrows move, d unmarks, Enter shows it in the list,",
    "  Ctrl-R deletes them all after you press Enter to confirm.",
    "",
    "Marking a directory covers its subtree and replaces marks inside it;",
    "  entries inside a marked directory cannot be marked separately.",
    "Deletion is permanent; there is no Trash. Byte totals are metadata",
    "  counts and released storage can differ.",
    "Rows: [x] marked (red)  [=] covered  ! incomplete  ~ stale  M mount  A alias",
    "  / directory  @ symlink (never followed)  * hardlinked name",
    "  Directory and symlink colors follow LS_COLORS; marked rows stay red.",
    "… means still scanning; ? no access means answer the macOS prompt.",
    "q or Ctrl-C quits. Esc or ? closes help.",
  ]
}

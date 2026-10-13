##! Builds bounded cell surfaces and computes recoverable terminal updates.
##!
##! Rows are sanitized and padded at construction, while the committed surface
##! changes only after the complete update has reached the output sink.
use tui
use btop_color as color
use btop_terminal as terminal

const MAX_DIMENSION = 2000
const MAX_CELLS = 200000

## Names a rectangle in terminal cells.
export type Rect = {x: Int, y: Int, width: Int, height: Int}

## Stores fixed-width sanitized display rows.
export type Surface = {width: Int, height: Int, lines: List[Str], roles: List[color.Role?]}

## Represents the screen state known to have been written in full.
export type Committed = {surface: Surface?, valid: Bool, color_depth: terminal.ColorDepth}

## Carries one desired frame and its full or delta terminal bytes.
export type Update = {surface: Surface, bytes: Str, full_redraw: Bool, color_depth: terminal.ColorDepth}

pure valid_dimensions(width: Int, height: Int) -> Bool {
  if width < 0 or height < 0 or width > MAX_DIMENSION or height > MAX_DIMENSION {
    return false
  }
  return true when width == 0
  height <= MAX_CELLS / width
}

pure repeated(character: Str, count: Int) -> Str {
  var result = ""
  for _ in range(count) { result += character }
  result
}

pure no_roles(height: Int) -> List[color.Role?] {
  var roles: List[color.Role?] = []
  for _ in range(height) { roles += [null] }
  roles
}

pure fit(value: Str, width: Int) -> Str {
  tui.cell_right_pad(tui.cell_clip(tui.safe_single_line(value), width), width)
}

## Creates a blank surface after checking its total cell bound.
export pure blank(width: Int, height: Int) -> Result[Surface, Error] {
  if ! valid_dimensions(width, height) {
    return Err(error.failure("surface dimensions exceed the cell bound"))
  }
  let line = repeated(" ", width)
  var lines: List[Str] = []
  for _ in range(height) { lines += [line] }
  Ok({width: width, height: height, lines: lines, roles: no_roles(height)})
}

## Builds a fixed-size surface, clipping and padding each row by display cells.
export pure from_lines(width: Int, height: Int, source: List[Str]) -> Result[Surface, Error] {
  if ! valid_dimensions(width, height) or source.len() > height {
    return Err(error.failure("surface rows do not fit the requested geometry"))
  }
  var lines: List[Str] = []
  for row in range(height) {
    let value = if row < source.len() { source[row] } else { "" }
    lines += [fit(value, width)]
  }
  Ok({width: width, height: height, lines: lines, roles: no_roles(height)})
}

## Adds one semantic palette role per output row while keeping source text inert.
export pure from_lines_with_roles(
  width: Int,
  height: Int,
  source: List[Str],
  roles: List[color.Role?],
) -> Result[Surface, Error] {
  let plain = from_lines(width, height, source)?
  if roles.len() > height {
    return Err(error.failure("surface palette roles exceed the row bound"))
  }
  var row_roles: List[color.Role?] = []
  for row in range(height) {
    row_roles += [if row < roles.len() { roles[row] } else { null }]
  }
  Ok({width: plain.width, height: plain.height, lines: plain.lines, roles: row_roles})
}

## Builds a clipped title and body inside a complete rectangular border.
export pure bordered(width: Int, height: Int, title: Str, body: List[Str]) -> Result[Surface, Error] {
  if width < 2 or height < 3 or ! valid_dimensions(width, height) {
    return Err(error.failure("bordered surface requires at least 2x3 cells"))
  }
  let inner_width = width - 2
  let edge = "+" + repeated("-", inner_width) + "+"
  var lines = [edge, "|" + fit(title, inner_width) + "|"]
  for row in range(height - 3) {
    let value = if row < body.len() { body[row] } else { "" }
    lines += ["|" + fit(value, inner_width) + "|"]
  }
  lines += [edge]
  from_lines(width, height, lines)
}

pure valid_rect(rect: Rect) -> Bool {
  rect.width >= 0 and rect.height >= 0 and
    rect.width <= MAX_DIMENSION and rect.height <= MAX_DIMENSION and
    rect.x >= -MAX_DIMENSION and rect.x <= MAX_DIMENSION and
    rect.y >= -MAX_DIMENSION and rect.y <= MAX_DIMENSION
}

## Returns the cell intersection or null for invalid or disjoint rectangles.
export pure intersect(left: Rect, right: Rect) -> Rect? {
  return null when ! valid_rect(left) or ! valid_rect(right)
  let x = if left.x > right.x { left.x } else { right.x }
  let y = if left.y > right.y { left.y } else { right.y }
  let right_edge = if left.x + left.width < right.x + right.width { left.x + left.width } else { right.x + right.width }
  let bottom_edge = if left.y + left.height < right.y + right.height { left.y + left.height } else { right.y + right.height }
  return null when x >= right_edge or y >= bottom_edge
  {x: x, y: y, width: right_edge - x, height: bottom_edge - y}
}

## Clips a surface to its intersection with a cell rectangle.
export pure clip_rect(surface: Surface, rectangle: Rect) -> Result[Surface, Error] {
  if ! valid_rect(rectangle) {
    return Err(error.failure("surface clip rectangle exceeds the geometry bound"))
  }
  let clipped = intersect(
    {x: 0, y: 0, width: surface.width, height: surface.height},
    rectangle,
  )
  return blank(0, 0) when clipped == null
  let area = clipped ?? {x: 0, y: 0, width: 0, height: 0}
  var lines: List[Str] = []
  for y in range(area.y, area.y + area.height) {
    lines += [tui.cell_right_pad(tui.cell_slice(surface.lines[y], area.x, area.width), area.width)]
  }
  var roles: List[color.Role?] = []
  for y in range(area.y, area.y + area.height) {
    roles += [if y < surface.roles.len() { surface.roles[y] } else { null }]
  }
  Ok({width: area.width, height: area.height, lines: lines, roles: roles})
}

## Draws a bounded ASCII meter; missing or non-finite values use question marks.
export pure meter(value: Float?, width: Int) -> Str {
  return "" when width <= 0 or width > MAX_DIMENSION
  return "?" + repeated(" ", width - 1) when value == null
  let amount = value ?? 0.0
  return "?" + repeated(" ", width - 1) when amount != amount
  let bounded = if amount < 0.0 { 0.0 } else if amount > 100.0 { 100.0 } else { amount }
  let filled = (bounded * width.float() / 100.0).floor() ?? 0
  repeated("#", filled) + repeated("-", width - filled)
}

## Builds one padded table row from columns with explicit cell widths.
export pure table_row(columns: List[Str], widths: List[Int], separator: Str = " ") -> Result[Str, Error] {
  if columns.len() != widths.len() {
    return Err(error.failure("table column and width counts differ"))
  }
  var row = ""
  for index in range(columns.len()) {
    let width = widths[index]
    if width < 0 or width > MAX_DIMENSION {
      return Err(error.failure("table column width exceeds the cell bound"))
    }
    if index > 0 { row += tui.safe_single_line(separator) }
    row += fit(columns[index], width)
  }
  Ok(row)
}

## Adds a selection marker and clips the value to one terminal row.
export pure selected_row(value: Str, selected: Bool, width: Int) -> Str {
  return "" when width <= 0
  let marker = if selected { "> " } else { "  " }
  return if selected { ">" } else { " " } when width == 1
  tui.cell_right_pad(marker + tui.cell_clip(tui.safe_single_line(value), width - 2), width)
}

## Returns a bounded viewport with blank rows after the visible input ends.
export pure viewport(rows: List[Str], offset: Int, height: Int) -> List[Str] {
  return [] when height <= 0 or height > MAX_DIMENSION
  let start = if offset < 0 { 0 } else { offset }
  var visible: List[Str] = []
  for row in range(height) {
    let at = start + row
    visible += [if at < rows.len() { rows[at] } else { "" }]
  }
  visible
}

## Shows a cell-positioned cursor while scrolling long input through a fixed field.
export pure input_field(value: Str, cursor_cell: Int, width: Int) -> Str {
  return "" when width <= 0 or width > MAX_DIMENSION
  let safe = tui.safe_single_line(value)
  let cursor = if cursor_cell < 0 { 0 } else { cursor_cell }
  let start = if cursor >= width { cursor - width + 1 } else { 0 }
  let visible = tui.cell_slice(safe, start, width)
  let cursor_at = cursor - start
  let before = tui.cell_slice(visible, 0, cursor_at)
  let after = tui.cell_slice(visible, cursor_at + 1, width - cursor_at - 1)
  tui.cell_right_pad(before + "_" + after, width)
}

## Represents a screen with no known fully committed frame.
export pure initial() -> Committed {
  {surface: null, valid: false, color_depth: .NoColor}
}

pure line_bytes(surface: Surface, row: Int, depth: terminal.ColorDepth) -> Str {
  let prefix = if row < surface.roles.len() {
    if let role = surface.roles[row] { color.foreground(role, depth) } else { "" }
  } else { "" }
  f"{prefix}{surface.lines[row]}\u{1b}[K\u{1b}[0m"
}

pure full_bytes(surface: Surface, depth: terminal.ColorDepth) -> Str {
  var output = "\u{1b}[2J"
  for row in range(surface.height) {
    output += f"\u{1b}[{row + 1};1H" + line_bytes(surface, row, depth)
  }
  output
}

## Computes changed rows without advancing the committed screen.
export pure prepare(
  committed: Committed,
  desired: Surface,
  depth: terminal.ColorDepth = .NoColor,
) -> Update {
  if ! committed.valid or committed.surface == null {
    return {surface: desired, bytes: full_bytes(desired, depth), full_redraw: true, color_depth: depth}
  }
  let previous = committed.surface ?? desired
  if previous.width != desired.width or previous.height != desired.height or committed.color_depth != depth {
    return {surface: desired, bytes: full_bytes(desired, depth), full_redraw: true, color_depth: depth}
  }
  var output = ""
  for row in range(desired.height) {
    let previous_role = if row < previous.roles.len() { previous.roles[row] } else { null }
    let desired_role = if row < desired.roles.len() { desired.roles[row] } else { null }
    if previous.lines[row] != desired.lines[row] or previous_role != desired_role {
      output += f"\u{1b}[{row + 1};1H" + line_bytes(desired, row, depth)
    }
  }
  {surface: desired, bytes: output, full_redraw: false, color_depth: depth}
}

## Commits only a complete write; partial and abandoned updates require redraw.
export pure complete(update: Update, written_bytes: Int) -> Committed {
  let expected = bytes.from_text(update.bytes).len()
  if written_bytes != expected {
    return {surface: null, valid: false, color_depth: update.color_depth}
  }
  {surface: update.surface, valid: true, color_depth: update.color_depth}
}

## Invalidates a previously committed frame after an abandoned or failed update.
export pure invalidate(committed: Committed) -> Committed {
  {surface: committed.surface, valid: false, color_depth: committed.color_depth}
}

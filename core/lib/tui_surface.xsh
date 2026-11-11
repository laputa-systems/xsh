##! Builds bounded text surfaces, cell screens, and recoverable terminal updates.
##!
##! Row surfaces sanitize and pad at construction and commit only after the
##! complete update reaches the output sink. Cell screens preserve wide-glyph
##! cells so erasing one half repaints the whole glyph, which overlays need.
use tui
use tui_colors as colors

const MAX_DIMENSION = 2000
const MAX_CELLS = 200000
const MODAL_WIDTH_PERCENT = 94

## Names a rectangle in terminal cells.
export type Rect = {x: Int, y: Int, width: Int, height: Int}

## Stores fixed-width sanitized display rows with per-row opaque style tokens.
export type Surface = {width: Int, height: Int, lines: List[Str], styles: List[Str]}

## Represents the screen state known to have been written in full.
export type Committed = {surface: Surface?, valid: Bool, style_key: Str}

## Carries one desired frame and its full or delta terminal bytes.
export type Update = {surface: Surface, bytes: Str, full_redraw: Bool, style_key: Str}

## Pairs one text glyph with its SGR style and display width.
export type Cell = {text: Str, style: Str, width: Int}

## Stores a complete cell grid; an empty text cell continues a wide glyph.
export type CellScreen = {
  width: Int,
  height: Int,
  texts: List[List[Str]],
  styles: List[List[Str]],
}

## Pairs a bordered cell screen with the inner area a caller may fill.
export type BorderedCells = {screen: CellScreen, inner: Rect}

pure valid_dimensions(width: Int, height: Int) -> Bool {
  if width < 0 or height < 0 or width > MAX_DIMENSION or height > MAX_DIMENSION {
    return false
  }
  return true when width == 0
  height <= MAX_CELLS / width
}

## Returns one character repeated a non-negative number of times.
export pure repeated(character: Str, count: Int) -> Str {
  var result = ""
  for _ in range(count) { result += character }
  result
}

pure no_styles(height: Int) -> List[Str] {
  let styles: List[Str] = collect {
    for _ in range(height) { yield "" }
  }
  styles
}

## Clips and right-pads text to a display-cell width.
export pure fit(value: Str, width: Int) -> Str {
  tui.cell_right_pad(tui.cell_clip(tui.safe_single_line(value), width), width)
}

## Creates a blank row surface after checking its total cell bound.
export pure blank(width: Int, height: Int) -> Result[Surface, Error] {
  guard valid_dimensions(width, height) else {
    return Err(error.failure("surface dimensions exceed the cell bound"))
  }
  let line = repeated(" ", width)
  let lines = collect {
    for _ in range(height) { yield line }
  }
  Ok({width: width, height: height, lines: lines, styles: no_styles(height)})
}

## Builds a fixed-size row surface, clipping and padding each row by cells.
export pure from_lines(width: Int, height: Int, source: List[Str]) -> Result[Surface, Error] {
  if ! valid_dimensions(width, height) or source.len() > height {
    return Err(error.failure("surface rows do not fit the requested geometry"))
  }
  let lines = collect {
    for row in range(height) {
      let value = if row < source.len() { source[row] } else { "" }
      yield fit(value, width)
    }
  }
  Ok({width: width, height: height, lines: lines, styles: no_styles(height)})
}

## Adds one opaque style token per output row while keeping source text inert.
export pure from_lines_with_styles(
  width: Int,
  height: Int,
  source: List[Str],
  styles: List[Str],
) -> Result[Surface, Error] {
  let plain = from_lines(width, height, source)?
  if styles.len() > height {
    return Err(error.failure("surface style tokens exceed the row bound"))
  }
  let row_styles: List[Str] = collect {
    for row in range(height) {
      yield if row < styles.len() { styles[row] } else { "" }
    }
  }
  Ok({width: plain.width, height: plain.height, lines: plain.lines, styles: row_styles})
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

## Draws a title centered in a rounded top border.
export pure top_border(width: Int, title: Str) -> Str {
  let inner_width = width - 2
  let label = f" {title} "
  let spare = inner_width - label.count_chars()
  let left = if spare < 0 { 0 } else { spare / 2 }
  let right = if spare < 0 { 0 } else { spare - left }
  f"╭{repeated("─", left)}{label}{repeated("─", right)}╮"
}

## Draws a rounded bottom border.
export pure bottom_border(width: Int) -> Str {
  f"╰{repeated("─", width - 2)}╯"
}

## Draws the monitor panel shape around fixed-width content rows.
export pure panel(width: Int, height: Int, title: Str, content: List[Str]) -> List[Str] {
  var rows = [top_border(width, title)]
  for index in range(height - 2) {
    let text = if index < content.len() { content[index] } else { "" }
    rows += [f"│{fit(text, width - 2)}│"]
  }
  rows += [bottom_border(width)]
  rows
}

## Draws two fixed-width content columns inside one bordered panel.
export pure split_panel(
  width: Int,
  height: Int,
  title: Str,
  left_content: List[Str],
  right_content: List[Str],
) -> List[Str] {
  let left_width = (width - 3) / 2
  let right_width = width - 3 - left_width
  var rows = [top_border(width, title)]
  for index in range(height - 2) {
    let left = if index < left_content.len() { left_content[index] } else { "" }
    let right = if index < right_content.len() { right_content[index] } else { "" }
    rows += [f"│{fit(left, left_width)}│{fit(right, right_width)}│"]
  }
  rows += [bottom_border(width)]
  rows
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

## Clips a row surface to its intersection with a cell rectangle.
export pure clip_rect(surface: Surface, rectangle: Rect) -> Result[Surface, Error] {
  guard valid_rect(rectangle) else {
    return Err(error.failure("surface clip rectangle exceeds the geometry bound"))
  }
  let clipped = intersect(
    {x: 0, y: 0, width: surface.width, height: surface.height},
    rectangle,
  )
  guard let area = clipped else {
    return blank(0, 0)
  }
  let lines = [
    tui.cell_right_pad(tui.cell_slice(surface.lines[y], area.x, area.width), area.width)
    for y in range(area.y, area.y + area.height)
  ]
  var styles: List[Str] = []
  for y in range(area.y, area.y + area.height) {
    styles += [if y < surface.styles.len() { surface.styles[y] } else { "" }]
  }
  Ok({width: area.width, height: area.height, lines: lines, styles: styles})
}

## Splits a rectangle into two columns at a rounded percentage of its width.
export pure split_columns(area: Rect, percent: Int) -> List[Rect] {
  let width = (area.width * percent + 50) / 100
  [
    {x: area.x, y: area.y, width: width, height: area.height},
    {x: area.x + width, y: area.y, width: area.width - width, height: area.height},
  ]
}

## Splits a rectangle into two rows at a rounded percentage of its height.
export pure split_rows(area: Rect, percent: Int) -> List[Rect] {
  let height = (area.height * percent + 50) / 100
  [
    {x: area.x, y: area.y, width: area.width, height: height},
    {x: area.x, y: area.y + height, width: area.width, height: area.height - height},
  ]
}

## Draws a bounded ASCII meter; missing or non-finite values use question marks.
export pure meter(value: Float?, width: Int) -> Str {
  return "" when width <= 0 or width > MAX_DIMENSION
  guard let amount = value else {
    return "?" + repeated(" ", width - 1)
  }
  return "?" + repeated(" ", width - 1) when amount != amount
  let bounded = if amount < 0.0 { 0.0 } else if amount > 100.0 { 100.0 } else { amount }
  let filled = (bounded * width.float() / 100.0).floor() ?? 0
  repeated("#", filled) + repeated("-", width - filled)
}

## Builds one padded table row from columns with explicit cell widths.
export pure table_row(columns: List[Str], widths: List[Int], separator = " ") -> Result[Str, Error] {
  guard columns.len() == widths.len() else {
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
  let visible = collect {
    for row in range(height) {
      let at = start + row
      yield if at < rows.len() { rows[at] } else { "" }
    }
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
  {surface: null, valid: false, style_key: ""}
}

pure line_bytes(surface: Surface, row: Int) -> Str {
  let prefix = if row < surface.styles.len() { surface.styles[row] } else { "" }
  f"{prefix}{surface.lines[row]}\u{1b}[K\u{1b}[0m"
}

pure full_bytes(surface: Surface) -> Str {
  var output = "\u{1b}[2J"
  for row in range(surface.height) {
    output += f"\u{1b}[{row + 1};1H" + line_bytes(surface, row)
  }
  output
}

## Computes changed rows without advancing the committed screen.
export pure prepare(
  committed: Committed,
  desired: Surface,
  style_key: Str = "",
) -> Update {
  if ! committed.valid or committed.surface == null {
    return {surface: desired, bytes: full_bytes(desired), full_redraw: true, style_key: style_key}
  }
  let previous = committed.surface ?? desired
  if previous.width != desired.width or previous.height != desired.height or committed.style_key != style_key {
    return {surface: desired, bytes: full_bytes(desired), full_redraw: true, style_key: style_key}
  }
  var output = ""
  for row in range(desired.height) {
    let previous_style = if row < previous.styles.len() { previous.styles[row] } else { "" }
    let desired_style = if row < desired.styles.len() { desired.styles[row] } else { "" }
    if previous.lines[row] != desired.lines[row] or previous_style != desired_style {
      output += f"\u{1b}[{row + 1};1H" + line_bytes(desired, row)
    }
  }
  {surface: desired, bytes: output, full_redraw: false, style_key: style_key}
}

## Commits only a complete write; partial and abandoned updates require redraw.
export pure complete(update: Update, written_bytes: Int) -> Committed {
  let expected = bytes.from_text(update.bytes).len()
  if written_bytes != expected {
    return {surface: null, valid: false, style_key: update.style_key}
  }
  {surface: update.surface, valid: true, style_key: update.style_key}
}

## Invalidates a previously committed frame after an abandoned or failed update.
export pure invalidate(committed: Committed) -> Committed {
  {surface: committed.surface, valid: false, style_key: committed.style_key}
}

## Splits text into terminal cells, keeping combining and joined glyphs intact.
export pure glyph_clusters(text: Str) -> List[Str] {
  let safe = tui.safe_single_line(text)
  var glyph = ""
  let clusters: List[Str] = collect {
    for character in safe {
      if glyph == "" {
        glyph = character
        continue
      }
      if glyph.byte_len() == 1 and character.byte_len() == 1 {
        yield glyph
        glyph = character
        continue
      }
      let before = tui.cell_width(glyph)
      let after = tui.cell_width(glyph + character)
      if tui.cell_width(character) == 0 or after <= before {
        glyph += character
      } else {
        yield glyph
        glyph = character
      }
    }
    if glyph != "" { yield glyph }
  }
  clusters
}

pure hex_byte(value: Int) -> Str {
  let digits = "0123456789abcdef"
  "\\x" + digits.byte_slice(value / 16, length: 1) + digits.byte_slice(value % 16, length: 1)
}

pure hex_number(value: Int) -> Str {
  let digits = "0123456789abcdef"
  if value == 0 { return "0" }
  var remaining = value
  var output = ""
  while remaining > 0 {
    output = digits.byte_slice(remaining % 16, length: 1) + output
    remaining = remaining / 16
  }
  output
}

pure codepoint_value(value: Str) -> Int {
  let raw = bytes.from_text(value)
  let first = raw.byte_at(0) ?? 0
  return first when first < 128
  let second = (raw.byte_at(1) ?? 128) - 128
  return (first - 192) * 64 + second when raw.len() == 2
  let third = (raw.byte_at(2) ?? 128) - 128
  return (first - 224) * 4096 + second * 64 + third when raw.len() == 3
  let third_byte = (raw.byte_at(2) ?? 128) - 128
  let fourth_byte = (raw.byte_at(3) ?? 128) - 128
  (first - 240) * 262144 + second * 4096 + third_byte * 64 + fourth_byte
}

pure utf8_size(first: Int) -> Int {
  return 2 when first >= 194 and first <= 223
  return 3 when first >= 224 and first <= 239
  return 4 when first >= 240 and first <= 244
  0
}

pure escaped_text(value: Str) -> Str {
  var output = ""
  for character in value {
    let codepoint = codepoint_value(character)
    if codepoint <= 31 or (codepoint >= 127 and codepoint <= 159) {
      output += f"\\u{{{hex_number(codepoint)}}}"
    } else {
      output += character
    }
  }
  output
}

## Escapes control characters and malformed bytes while keeping valid text.
export pure escape_name(name: Bytes) -> Str {
  var output = ""
  var at = 0
  while at < name.len() {
    let lead = name.byte_at(at) ?? 0
    if lead < 128 {
      output += escaped_text(name[at..at + 1].utf8() ?? "")
      at += 1
      continue
    }
    let size = utf8_size(lead)
    if size == 0 or at + size > name.len() {
      output += hex_byte(lead)
      at += 1
    } else if let Ok(character) = name[at..at + size].utf8() {
      output += escaped_text(character)
      at += size
    } else {
      output += hex_byte(lead)
      at += 1
    }
  }
  output
}

## Shortens text to at most a cell width, ending with an ellipsis when cut.
export pure clip_end(text: Str, maximum: Int) -> Str {
  if tui.cell_width(text) <= maximum {
    return text
  }
  if maximum == 0 {
    return ""
  }
  var output = ""
  for cluster in glyph_clusters(text) {
    let candidate = output + cluster
    if tui.cell_width(candidate) > maximum - 1 { break }
    output = candidate
  }
  output + "…"
}

## Shortens text to at most a cell width while keeping its end.
export pure clip_start(text: Str, maximum: Int) -> Str {
  if tui.cell_width(text) <= maximum {
    return text
  }
  if maximum == 0 {
    return ""
  }
  let clusters = glyph_clusters(text)
  var kept: List[Str] = []
  var used = 0
  var index = clusters.len()
  while index > 0 {
    index -= 1
    let width = tui.cell_width(clusters[index])
    if used + width > maximum - 1 { break }
    kept = [clusters[index]] + kept
    used += width
  }
  "…" + kept.join("")
}

## Pads with spaces to exactly a cell width so a reversed row fills its pane.
export pure pad_to(text: Str, width: Int) -> Str {
  let used = tui.cell_width(text)
  return text when used >= width
  text + repeated(" ", width - used)
}

## Formats a byte count with one decimal in binary units.
export pure format_size(size: Int) -> Str {
  let units = ["B", "KiB", "MiB", "GiB", "TiB"]
  if size < 1024 {
    return f"{size} B"
  }
  var value = size.float()
  var unit = 0
  while value >= 1024.0 and unit < units.len() - 1 {
    value /= 1024.0
    unit += 1
  }
  f"{value.format(precision: 1)} {units[unit]}"
}

## Formats an age in seconds using one rounded unit, capping years at 999.
export pure format_age_seconds(seconds: Int) -> Str {
  let year_seconds = 365 * 24 * 60 * 60
  let units = [
    {seconds: year_seconds, suffix: "y"},
    {seconds: year_seconds / 12, suffix: "mo"},
    {seconds: 7 * 24 * 60 * 60, suffix: "w"},
    {seconds: 24 * 60 * 60, suffix: "d"},
    {seconds: 60 * 60, suffix: "h"},
    {seconds: 60, suffix: "m"},
    {seconds: 1, suffix: "s"},
  ]
  var unit_index = units.len() - 1
  for index in range(units.len()) {
    if seconds >= units[index].seconds {
      unit_index = index
      break
    }
  }
  while true {
    let unit = units[unit_index]
    let value = (seconds + unit.seconds / 2) / unit.seconds
    if unit_index == 0 {
      return if value > 999 { "999y+" } else { f"{value}{unit.suffix}" }
    }
    if value * unit.seconds >= units[unit_index - 1].seconds {
      unit_index -= 1
      continue
    }
    return f"{value}{unit.suffix}"
  }
}

## Formats an age from a modified time, or an em dash when it is unknown.
export pure format_age(modified_seconds: Int?, now_seconds: Int) -> Str {
  guard let modified = modified_seconds else {
    return "—"
  }
  let elapsed = if now_seconds > modified { now_seconds - modified } else { 0 }
  format_age_seconds(elapsed)
}

const PLAIN_STYLE = "\u{1b}[0m"

## Creates a blank cell screen after checking its total cell bound.
export pure cell_screen(width: Int, height: Int) -> Result[CellScreen, Error] {
  guard valid_dimensions(width, height) else {
    return Err(error.failure("cell screen dimensions exceed the cell bound"))
  }
  let texts: List[List[Str]] = collect {
    for _ in range(height) { yield [" " for _ in range(width)] }
  }
  let styles: List[List[Str]] = collect {
    for _ in range(height) { yield [PLAIN_STYLE for _ in range(width)] }
  }
  Ok({width: width, height: height, texts: texts, styles: styles})
}

pure cell_text_width(text: Str) -> Int {
  return 0 when text == ""
  return 1 when text.byte_len() == 1
  tui.cell_width(text)
}

## Returns the cell at a coordinate, or null outside the screen.
export pure cell_at(screen: CellScreen, x: Int, y: Int) -> Cell? {
  if x < 0 or y < 0 or x >= screen.width or y >= screen.height {
    return null
  }
  {
    text: screen.texts[y][x],
    style: screen.styles[y][x],
    width: cell_text_width(screen.texts[y][x]),
  }
}

type Span = {start: Int, stop: Int}

pure erase_span(
  row_texts: List[Str],
  screen_width: Int,
  start: Int,
  stop: Int,
) -> Span {
  var left = start
  var right = stop
  var changed = true
  while changed {
    changed = false
    while left > 0 and row_texts[left] == "" {
      left -= 1
      changed = true
    }
    for column in range(left, right) {
      let glyph_width = cell_text_width(row_texts[column])
      if glyph_width > 1 and column + glyph_width > right {
        let expanded = if column + glyph_width < screen_width { column + glyph_width } else { screen_width }
        if expanded > right {
          right = expanded
          changed = true
        }
      }
    }
    while right < screen_width and row_texts[right] == "" {
      right += 1
      changed = true
    }
  }
  {start: left, stop: right}
}

## Writes text into one cell row, clipping at the area edge by display cells.
export pure cell_text(screen: CellScreen, area: Rect, text: Str, style: colors.Style) -> CellScreen {
  if area.height == 0 or area.y < 0 or area.y >= screen.height {
    return screen
  }
  let right = area.x + area.width
  let end = if right < screen.width { right } else { screen.width }
  let start_x = if area.x < 0 { 0 } else { area.x }
  var x = start_x
  var glyph_texts: List[Str] = []
  for glyph in glyph_clusters(text) {
    let glyph_width = tui.cell_width(glyph)
    if glyph_width == 0 { continue }
    break when x + glyph_width > end
    glyph_texts += [glyph]
    for _ in range(1, glyph_width) {
      glyph_texts += [""]
    }
    x += glyph_width
  }
  if glyph_texts.is_empty() {
    return screen
  }
  let row_texts = screen.texts[area.y]
  let row_styles = screen.styles[area.y]
  let written = glyph_texts.len()
  let token = colors.ansi(style)
  let span = erase_span(row_texts, screen.width, start_x, start_x + written)
  let replaced_texts: List[Str] = collect {
    for column in range(span.start, span.stop) {
      let offset = column - start_x
      yield if offset >= 0 and offset < written { glyph_texts[offset] } else { " " }
    }
  }
  let replaced_styles: List[Str] = collect {
    for column in range(span.start, span.stop) {
      let offset = column - start_x
      yield if offset >= 0 and offset < written { token } else { PLAIN_STYLE }
    }
  }
  {
    ...screen,
    texts: screen.texts[..area.y] + [row_texts[..span.start] + replaced_texts + row_texts[span.stop..]] + screen.texts[area.y + 1..],
    styles: screen.styles[..area.y] + [row_styles[..span.start] + replaced_styles + row_styles[span.stop..]] + screen.styles[area.y + 1..],
  }
}

## Writes plain-styled lines starting at a cell area's top row.
export pure cell_lines(screen: CellScreen, area: Rect, lines: List[Str]) -> CellScreen {
  var updated = screen
  var offset = 0
  for line in lines {
    if offset >= area.height { break }
    updated = cell_text(updated, {x: area.x, y: area.y + offset, width: area.width, height: 1}, line, colors.PLAIN)
    offset += 1
  }
  updated
}

## Clears a cell area, erasing wide glyphs that cross its boundary.
export pure cell_clear(screen: CellScreen, area: Rect) -> CellScreen {
  let right = area.x + area.width
  let bottom = area.y + area.height
  let start_x = if area.x < 0 { 0 } else { area.x }
  let start_y = if area.y < 0 { 0 } else { area.y }
  let end_x = if right < screen.width { right } else { screen.width }
  let end_y = if bottom < screen.height { bottom } else { screen.height }
  if end_x <= start_x or end_y <= start_y {
    return screen
  }
  let texts: List[List[Str]] = collect {
    for y in range(screen.height) {
      if y >= start_y and y < end_y {
        let row = screen.texts[y]
        let span = erase_span(row, screen.width, start_x, end_x)
        let blanks: List[Str] = [" " for _ in range(span.stop - span.start)]
        yield row[..span.start] + blanks + row[span.stop..]
      } else {
        yield screen.texts[y]
      }
    }
  }
  let styles: List[List[Str]] = collect {
    for y in range(screen.height) {
      if y >= start_y and y < end_y {
        let row = screen.styles[y]
        let span = erase_span(screen.texts[y], screen.width, start_x, end_x)
        let blanks: List[Str] = [PLAIN_STYLE for _ in range(span.stop - span.start)]
        yield row[..span.start] + blanks + row[span.stop..]
      } else {
        yield screen.styles[y]
      }
    }
  }
  {...screen, texts: texts, styles: styles}
}

## Draws a complete rectangular border and returns the inner fill area.
export pure cell_border(
  screen: CellScreen,
  area: Rect,
  title: Str,
  border: colors.Style,
  title_style: colors.Style,
) -> BorderedCells {
  if area.width == 0 or area.height == 0 {
    return {screen: screen, inner: {x: area.x, y: area.y, width: 0, height: 0}}
  }
  let right = area.x + area.width - 1
  let bottom = area.y + area.height - 1
  var updated = screen
  for y in range(area.y, bottom + 1) {
    updated = cell_text(updated, {x: area.x, y: y, width: 1, height: 1}, "│", border)
    updated = cell_text(updated, {x: right, y: y, width: 1, height: 1}, "│", border)
  }
  let horizontal = repeated("─", area.width)
  updated = cell_text(updated, {x: area.x, y: area.y, width: area.width, height: 1}, horizontal, border)
  updated = cell_text(updated, {x: area.x, y: bottom, width: area.width, height: 1}, horizontal, border)
  updated = cell_text(updated, {x: area.x, y: area.y, width: 1, height: 1}, "┌", border)
  updated = cell_text(updated, {x: right, y: area.y, width: 1, height: 1}, "┐", border)
  updated = cell_text(updated, {x: area.x, y: bottom, width: 1, height: 1}, "└", border)
  updated = cell_text(updated, {x: right, y: bottom, width: 1, height: 1}, "┘", border)
  updated = cell_text(
    updated,
    {x: area.x + 1, y: area.y, width: if area.width > 2 { area.width - 2 } else { 0 }, height: 1},
    title,
    title_style,
  )
  {
    screen: updated,
    inner: {
      x: area.x + 1,
      y: area.y + 1,
      width: if area.width > 2 { area.width - 2 } else { 0 },
      height: if area.height > 2 { area.height - 2 } else { 0 },
    },
  }
}

## Emits the bytes needed to change one cell screen into another.
export pure cell_diff(previous: CellScreen, next: CellScreen) -> Str {
  let resized = next.width != previous.width or next.height != previous.height
  var cursor_x = -1
  var cursor_y = -1
  var style = PLAIN_STYLE
  let parts: List[Str] = collect {
    if resized { yield "\u{1b}[0m\u{1b}[2J" }
    for y in range(next.height) {
      let next_texts = next.texts[y]
      let next_styles = next.styles[y]
      let previous_texts = if resized { next_texts } else { previous.texts[y] }
      let previous_styles = if resized { next_styles } else { previous.styles[y] }
      for x in range(next.width) {
        let glyph_text = next_texts[x]
        if glyph_text == "" {
          continue
        }
        let cell_style = next_styles[x]
        if resized and glyph_text == " " and cell_style == PLAIN_STYLE {
          continue
        }
        if ! resized and glyph_text == previous_texts[x] and cell_style == previous_styles[x] {
          continue
        }
        if cursor_x != x or cursor_y != y {
          yield f"\u{1b}[{y + 1};{x + 1}H"
        }
        if style != cell_style {
          yield cell_style
          style = cell_style
        }
        yield glyph_text
        cursor_x = x + cell_text_width(glyph_text)
        cursor_y = y
      }
    }
    if style != PLAIN_STYLE { yield PLAIN_STYLE }
  }
  return "" when parts.is_empty()
  parts.join("") + "\u{1b}[1;1H"
}

## Returns the plain text of every cell row, for assertions and diagnostics.
export pure cell_plain(screen: CellScreen) -> Str {
  let rows = collect {
    for y in range(screen.height) {
      var row = ""
      for text in screen.texts[y] {
        row += text
      }
      yield row
    }
  }
  rows.join("\n")
}

## Centers a modal rectangle in an area at a percentage width and given height.
export pure modal_area(area: Rect, width_percent: Int, height: Int) -> Rect {
  var width = area.width * width_percent / 100
  if width < 1 { width = 1 }
  if width > area.width { width = area.width }
  let bounded_height = if height < area.height { height } else { area.height }
  {
    x: area.x + (area.width - width) / 2,
    y: area.y + (area.height - bounded_height) / 2,
    width: width,
    height: bounded_height,
  }
}

## Clears a centered modal area, draws its border and title, then its lines.
export pure cell_modal(screen: CellScreen, title: Str, lines: List[Str]) -> Result[CellScreen, Error] {
  let wanted = if lines.len() + 2 > 7 { lines.len() + 2 } else { 7 }
  let height = if wanted < screen.height { wanted } else { screen.height }
  let area = modal_area({x: 0, y: 0, width: screen.width, height: screen.height}, MODAL_WIDTH_PERCENT, height)
  let cleared = cell_clear(screen, area)
  let framed = cell_border(cleared, area, title, colors.PLAIN, colors.PLAIN)
  Ok(cell_lines(framed.screen, framed.inner, lines))
}

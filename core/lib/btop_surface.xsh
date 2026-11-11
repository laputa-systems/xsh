##! Maps the monitor's palette roles onto generic row surfaces.
##!
##! Surface geometry, clipping, widgets, and commit tracking come from
##! [tui_surface](tui_surface.xsh). This module keeps the monitor's per-row
##! palette roles and renders them at the captured color depth.
use btop_color as color
use btop_terminal as terminal
use tui_surface as surface

## Names a rectangle in terminal cells.
export type Rect = surface.Rect

## Stores fixed-width sanitized display rows and their palette tokens.
export type Surface = surface.Surface

## Represents the screen state known to have been written in full.
export type Committed = surface.Committed

## Carries one desired frame and its full or delta terminal bytes.
export type Update = surface.Update

pure role_token(role: color.Role?) -> Str {
  if let value = role {
    match value {
      color.Cpu => "cpu"
      color.Memory => "memory"
      color.Network => "network"
      color.Process => "process"
    }
  } else {
    ""
  }
}

pure role_prefix(token: Str, depth: terminal.ColorDepth) -> Str {
  match token {
    "cpu" => color.foreground(color.Cpu, depth)
    "memory" => color.foreground(color.Memory, depth)
    "network" => color.foreground(color.Network, depth)
    "process" => color.foreground(color.Process, depth)
    else => ""
  }
}

pure depth_key(depth: terminal.ColorDepth) -> Str {
  match depth {
    terminal.NoColor => "none"
    terminal.Ansi16 => "ansi16"
    terminal.Ansi256 => "ansi256"
    terminal.TrueColor => "truecolor"
  }
}

## Returns the cell intersection or null for invalid or disjoint rectangles.
export pure intersect(left: Rect, right: Rect) -> Rect? {
  surface.intersect(left, right)
}

## Creates a blank surface after checking its total cell bound.
export pure blank(width: Int, height: Int) -> Result[Surface, Error] {
  surface.blank(width, height)
}

## Builds a fixed-size surface, clipping and padding each row by display cells.
export pure from_lines(width: Int, height: Int, source: List[Str]) -> Result[Surface, Error] {
  surface.from_lines(width, height, source)
}

## Adds one palette role per output row while keeping source text inert.
export pure from_lines_with_roles(
  width: Int,
  height: Int,
  source: List[Str],
  roles: List[color.Role?],
) -> Result[Surface, Error] {
  if roles.len() > height {
    return Err(error.failure("surface palette roles exceed the row bound"))
  }
  let tokens: List[Str] = collect {
    for row in range(height) {
      yield if row < roles.len() { role_token(roles[row]) } else { "" }
    }
  }
  surface.from_lines_with_styles(width, height, source, tokens)
}

## Builds a clipped title and body inside a complete rectangular border.
export pure bordered(width: Int, height: Int, title: Str, body: List[Str]) -> Result[Surface, Error] {
  surface.bordered(width, height, title, body)
}

## Clips a surface to its intersection with a cell rectangle.
export pure clip_rect(value: Surface, rectangle: Rect) -> Result[Surface, Error] {
  surface.clip_rect(value, rectangle)
}

## Draws a bounded ASCII meter; missing or non-finite values use question marks.
export pure meter(value: Float?, width: Int) -> Str {
  surface.meter(value, width)
}

## Builds one padded table row from columns with explicit cell widths.
export pure table_row(columns: List[Str], widths: List[Int], separator = " ") -> Result[Str, Error] {
  surface.table_row(columns, widths, separator)
}

## Adds a selection marker and clips the value to one terminal row.
export pure selected_row(value: Str, selected: Bool, width: Int) -> Str {
  surface.selected_row(value, selected, width)
}

## Returns a bounded viewport with blank rows after the visible input ends.
export pure viewport(rows: List[Str], offset: Int, height: Int) -> List[Str] {
  surface.viewport(rows, offset, height)
}

## Shows a cell-positioned cursor while scrolling long input through a fixed field.
export pure input_field(value: Str, cursor_cell: Int, width: Int) -> Str {
  surface.input_field(value, cursor_cell, width)
}

## Represents a screen with no known fully committed frame.
export pure initial() -> Committed {
  surface.initial()
}

## Computes changed rows without advancing the committed screen.
export pure prepare(
  committed: Committed,
  desired: Surface,
  depth: terminal.ColorDepth = .NoColor,
) -> Update {
  let rendered = {
    ...desired,
    styles: [role_prefix(token, depth) for token in desired.styles],
  }
  surface.prepare(committed, rendered, depth_key(depth))
}

## Commits only a complete write; partial and abandoned updates require redraw.
export pure complete(update: Update, written_bytes: Int) -> Committed {
  surface.complete(update, written_bytes)
}

## Invalidates a previously committed frame after an abandoned or failed update.
export pure invalidate(committed: Committed) -> Committed {
  surface.invalidate(committed)
}

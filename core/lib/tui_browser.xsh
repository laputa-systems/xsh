##! Reduces key intents into a two-pane browser with focus and mark selection.
##!
##! The model names rows by opaque integer identity, so a rescan or a different
##! directory can change row positions without moving the cursor to another
##! entry. Marks keep their insertion order, and a range extends between the
##! anchor and the cursor as the cursor moves.
use tui_input as input

## Names which pane owns movement and marks.
export enum Pane { Browser, Marked }

## Arranges the two panes side by side or stacked.
export enum Split { Vertical, Horizontal }

## Names the browser transitions shared by file and object browsers.
export enum Intent {
  MoveUp, MoveDown, PageUp, PageDown, Home, End,
  ToggleMark, ToggleRange, MarkAll, ClearMarks,
  SwitchPane, ToggleSplit,
}

## Narrowest terminal that shows two side-by-side panes.
export const SPLIT_MIN_COLUMNS = 60
## Shortest terminal that shows two stacked panes.
export const STACK_MIN_ROWS = 12

## Names the first visible row and the scroll offset to retain.
export type Window = {start: Int, top: Int}

## Retains cursor identity, pane focus, split arrangement, and ordered marks.
export type State = {
  focus: Pane,
  split: Split,
  side_visible: Bool,
  cursor: Int?,
  marked_cursor: Int?,
  range_mode: Bool,
  range_anchor: Int?,
  marks: List[Int],
  marked_set: Set[Int],
  scroll: Int,
  marked_scroll: Int,
}

## Starts with the browser focused, one pane, and no marks.
export pure initial() -> State {
  {
    focus: Browser,
    split: Vertical,
    side_visible: false,
    cursor: null,
    marked_cursor: null,
    range_mode: false,
    range_anchor: null,
    marks: [],
    marked_set: set.empty(),
    scroll: 0,
    marked_scroll: 0,
  }
}

## Returns the opposite arrangement of a split.
export pure split_toggled(split: Split) -> Split {
  match split {
    Vertical => Horizontal
    Horizontal => Vertical
  }
}

## Reports whether the terminal can show both panes in the chosen arrangement.
export pure fits_two_panes(split: Split, columns: Int, rows: Int) -> Bool {
  match split {
    Vertical => columns >= SPLIT_MIN_COLUMNS
    Horizontal => rows >= STACK_MIN_ROWS
  }
}

## Reports whether the marked pane occupies space, including a narrow focus.
export pure side_visible(state: State, columns: Int, rows: Int) -> Bool {
  ! state.marks.is_empty() and (fits_two_panes(state.split, columns, rows) or state.focus == Marked)
}

## Keeps focus on the browser whenever nothing is marked and recomputes layout.
export pure normalize(state: State, columns: Int, rows: Int) -> State {
  let focus = if state.marks.is_empty() { Browser } else { state.focus }
  let visible = ! state.marks.is_empty() and
    (fits_two_panes(state.split, columns, rows) or focus == Marked)
  {...state, focus: focus, side_visible: visible}
}

pure membership(marks: List[Int]) -> Set[Int] {
  var found: Set[Int] = set.empty()
  for id in marks {
    found = found.add(id)
  }
  found
}

pure with_marks(state: State, marks: List[Int]) -> State {
  let marked_cursor = if marks.is_empty() {
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
  {...state, marks: marks, marked_set: membership(marks), marked_cursor: marked_cursor}
}

## Returns the cursor's row position, or null when the cursor is not listed.
export pure cursor_index(state: State, entries: List[Int]) -> Int? {
  if let cursor = state.cursor {
    for index in range(entries.len()) {
      return index when entries[index] == cursor
    }
  }
  null
}

## Keeps the cursor on a listed row after the row set changes.
export pure reconcile(state: State, entries: List[Int]) -> State {
  if entries.is_empty() {
    return {...state, cursor: null}
  }
  if cursor_index(state, entries) == null {
    return {...state, cursor: entries[0]}
  }
  state
}

## Selects one listed row without changing marks.
export pure select(state: State, entries: List[Int], id: Int) -> State {
  return state when cursor_index({...state, cursor: id}, entries) == null
  {...state, cursor: id}
}

pure extend_range(state: State, entries: List[Int]) -> State {
  if state.range_anchor == null or state.cursor == null {
    return state
  }
  let anchor = state.range_anchor ?? 0
  let target = state.cursor ?? 0
  var anchor_at: Int? = null
  var target_at: Int? = null
  for index in range(entries.len()) {
    if entries[index] == anchor { anchor_at = index }
    if entries[index] == target { target_at = index }
  }
  if anchor_at == null or target_at == null {
    return state
  }
  let first = anchor_at ?? 0
  let last = target_at ?? 0
  let start = if first < last { first } else { last }
  let stop = if first < last { last } else { first }
  var marks = state.marks
  for index in range(start, stop + 1) {
    if entries[index] not in state.marked_set {
      marks += [entries[index]]
    }
  }
  with_marks(state, marks)
}

## Moves the browser cursor, extending an active range as it travels.
export pure move_cursor(state: State, entries: List[Int], amount: Int) -> State {
  return state when entries.is_empty()
  let current = cursor_index(state, entries) ?? 0
  var target = current + amount
  if target < 0 {
    target = 0
  } else if target >= entries.len() {
    target = entries.len() - 1
  }
  var updated: State = {...state, cursor: entries[target]}
  if state.range_mode {
    updated = extend_range(updated, entries)
  }
  updated
}

## Moves the browser cursor to its first or last listed row.
export pure move_to_edge(state: State, entries: List[Int], end: Bool) -> State {
  return state when entries.is_empty()
  let target = if end { entries.len() - 1 } else { 0 }
  var updated: State = {...state, cursor: entries[target]}
  if state.range_mode {
    updated = extend_range(updated, entries)
  }
  updated
}

## Moves the marked-pane cursor within the ordered marks.
export pure move_marked_cursor(state: State, amount: Int) -> State {
  return state when state.marks.is_empty()
  let current = state.marked_cursor ?? 0
  var target = current + amount
  if target < 0 {
    target = 0
  } else if target >= state.marks.len() {
    target = state.marks.len() - 1
  }
  {...state, marked_cursor: target}
}

## Toggles the cursor row's mark, then advances the cursor like a file browser.
export pure toggle_mark(state: State, entries: List[Int]) -> State {
  guard let cursor = state.cursor else {
    return state
  }
  var marks = state.marks
  if cursor in state.marked_set {
    marks = [id for id in marks if id != cursor]
  } else {
    marks += [cursor]
  }
  move_cursor(with_marks(state, marks), entries, 1)
}

## Starts or ends range mode; starting marks the cursor row as the anchor.
export pure toggle_range(state: State) -> State {
  if state.range_mode {
    return {...state, range_mode: false, range_anchor: null}
  }
  guard let cursor = state.cursor else {
    return state
  }
  var marks = state.marks
  if cursor not in state.marked_set {
    marks += [cursor]
  }
  let marked = with_marks(state, marks)
  {...marked, range_mode: true, range_anchor: cursor}
}

## Marks every listed row that is not already marked.
export pure mark_all(state: State, entries: List[Int]) -> State {
  var marks = state.marks
  for id in entries {
    if id not in state.marked_set {
      marks += [id]
    }
  }
  with_marks(state, marks)
}

## Removes every mark and leaves range mode.
export pure clear_marks(state: State) -> State {
  {...with_marks(state, []), range_mode: false, range_anchor: null}
}

## Removes the marked-pane cursor's mark and clamps the pane cursor.
export pure unmark_at_cursor(state: State) -> State {
  let at = state.marked_cursor ?? 0
  if at < 0 or at >= state.marks.len() {
    return state
  }
  let remaining: List[Int] = collect {
    for index in range(state.marks.len()) {
      yield state.marks[index] when index != at
    }
  }
  with_marks(state, remaining)
}

## Switches focus when at least one entry is marked.
export pure switch_pane(state: State) -> State {
  return state when state.marks.is_empty()
  let focus = match state.focus {
    Browser => Marked
    Marked => Browser
  }
  {...state, focus: focus}
}

## Returns the marker text for a row's mark state.
export pure marker(marked: Bool, covered: Bool) -> Str {
  return "[x]" when marked
  return "[=]" when covered
  "   "
}

## Maps the browser keys shared by file and object browsers to intents.
export pure browse_intent(key: input.Key) -> Intent? {
  match key {
    input.Up | input.Text("k") => MoveUp
    input.Down | input.Text("j") => MoveDown
    input.PageUp => PageUp
    input.PageDown => PageDown
    input.Home => Home
    input.End => End
    input.Tab => SwitchPane
    input.Text("d") => ToggleMark
    input.Text("v") => ToggleRange
    input.Text("*") => MarkAll
    input.Text("c") => ClearMarks
    input.Text("-") => ToggleSplit
    else => null
  }
}

## Applies one intent and recomputes focus and layout for the terminal size.
export pure apply(
  state: State,
  intent: Intent,
  entries: List[Int],
  columns: Int,
  rows: Int,
) -> State {
  var updated = state
  if intent == SwitchPane {
    updated = switch_pane(state)
  } else if intent == ToggleSplit {
    updated = {...state, split: split_toggled(state.split)}
  } else if intent == ClearMarks {
    updated = clear_marks(state)
  } else if intent == MarkAll {
    updated = mark_all(state, entries)
  } else if state.focus == Marked {
    updated = match intent {
      MoveUp => move_marked_cursor(state, -1)
      MoveDown => move_marked_cursor(state, 1)
      PageUp => move_marked_cursor(state, -15)
      PageDown => move_marked_cursor(state, 15)
      Home => move_marked_cursor(state, -state.marks.len())
      End => move_marked_cursor(state, state.marks.len())
      ToggleMark => unmark_at_cursor(state)
      else => state
    }
  } else {
    let page = if rows < 1 { 1 } else { rows }
    updated = match intent {
      MoveUp => move_cursor(state, entries, -1)
      MoveDown => move_cursor(state, entries, 1)
      PageUp => move_cursor(state, entries, -page)
      PageDown => move_cursor(state, entries, page)
      Home => move_to_edge(state, entries, false)
      End => move_to_edge(state, entries, true)
      ToggleMark => toggle_mark(state, entries)
      ToggleRange => toggle_range(state)
      else => state
    }
  }
  normalize(updated, columns, rows)
}

## Returns the first visible row and the retained scroll offset.
export pure window_start(top: Int, cursor: Int?, row_count: Int, available_rows: Int) -> Window {
  let maximum = if row_count > available_rows { row_count - available_rows } else { 0 }
  var start = if top < maximum { top } else { maximum }
  if let index = cursor {
    if index < row_count {
      if index < start {
        start = index
      } else if available_rows > 0 and index >= start + available_rows {
        start = index + 1 - available_rows
      }
    }
  }
  {start: start, top: start}
}

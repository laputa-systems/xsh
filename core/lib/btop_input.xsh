##! Keeps the monitor's input names and events over the generic TUI decoder.
##!
##! The decoder lives in [tui_input](tui_input.xsh). These names stay because
##! the process view, its tests, and the applet are written against them.
use tui_input as generic

## Maximum bytes retained while an escape sequence is being decoded.
export const MAX_SEQUENCE_BYTES = generic.MAX_SEQUENCE_BYTES
## Maximum paste payload retained before the paste event is marked truncated.
export const MAX_PASTE_BYTES = generic.MAX_PASTE_BYTES
## Delay after Escape before a lone Escape key is emitted.
export const ESCAPE_TIMEOUT_MS = generic.ESCAPE_TIMEOUT_MS

## A decoded key; text is one valid Unicode scalar and controls stay explicit.
export enum Key {
  Text(Str), Control(Int), Enter, Escape, Backspace, Tab, Cancel,
  Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Delete,
  Function(Int),
}

## Paste stays grouped so its contents cannot dispatch individual hotkeys.
export enum Event { Pressed(Key), Paste(Str, Bool), Eof }

## Pending bytes and bounded paste state for an incremental decode.
export type State = generic.State

## State and all events produced by one feed, timeout, or finish operation.
export type Step = {state: generic.State, events: List[Event]}

pure convert_key(key: generic.Key) -> Key {
  match key {
    generic.Text(text) => Text(text)
    generic.Control(value) => Control(value)
    generic.Enter => Enter
    generic.Escape => Escape
    generic.Backspace => Backspace
    generic.Tab => Tab
    generic.Cancel => Cancel
    generic.Up => Up
    generic.Down => Down
    generic.Left => Left
    generic.Right => Right
    generic.Home => Home
    generic.End => End
    generic.PageUp => PageUp
    generic.PageDown => PageDown
    generic.Insert => Insert
    generic.Delete => Delete
    generic.Function(number) => Function(number)
  }
}

pure convert_event(event: generic.Event) -> Event {
  match event {
    generic.Pressed(key) => Pressed(convert_key(key))
    generic.Paste(paste) => Paste(paste.text, paste.truncated)
    generic.Eof => Eof
  }
}

pure convert_step(step: generic.Step) -> Step {
  {state: step.state, events: [convert_event(event) for event in step.events]}
}

## Start with no pending input.
export pure initial() -> State {
  generic.initial()
}

## Feed any number of bytes; complete events are emitted in input order.
export pure push(state: State, chunk: Bytes, now_ms: Int) -> Step {
  convert_step(generic.push(state, chunk, now_ms))
}

## Advance a replayable clock; equality with the deadline expires Escape.
export pure timeout(state: State, now_ms: Int) -> Step {
  convert_step(generic.timeout(state, now_ms))
}

## Reports how long a poll may block: the limit, shortened to the pending Escape deadline and never below one millisecond.
export pure wait_ms(state: State, now_ms: Int, limit_ms: Int) -> Int {
  generic.wait_ms(state, now_ms, limit_ms)
}

## Flush incomplete text safely, discard incomplete controls, then report EOF.
export pure finish(state: State) -> Step {
  convert_step(generic.finish(state))
}

## Returns bytes fed but not decoded yet, for hand-off to a parent reader.
export pure unconsumed(state: State) -> Bytes {
  generic.unconsumed(state)
}

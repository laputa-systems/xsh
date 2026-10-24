use effects
let selected = effects.pair
proc clock(value: Int) [time] -> Int { let _ = time.now(); value }
proc emit(value: Int) [io] -> Int { print "integer callback"; value }
proc clock_text(value: Str) [time] -> Str { let _ = time.now(); value }
proc emit_text(value: Str) [io] -> Str { print "text callback"; value }

proc verify_effect_union() [time, io, error] -> Result[Unit] {
  let number = selected(clock, emit, 7)?
  let text = selected(clock_text, emit_text, "seven")?
  assert number == {left: 7, right: 7}
  assert text == {left: "seven", right: "seven"}
  print "union preserved"
}

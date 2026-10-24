use guarded-observables as guarded

type Snapshot = {events: List[Str], calls: Int, cleaned: Int}
error GuardedCallbackFailure = Rejected(message: Str)
var events: List[Str] = []
var calls = 0
var cleaned = 0

proc reset() [] -> Unit { events = []; calls = 0; cleaned = 0 }
pure snapshot() -> Snapshot { {events, calls, cleaned} }
proc argument(value: Int) [] -> Int { events += ["argument"]; value }
proc observe(value: Int) [io, error] -> Result[Int, Error] {
  calls += 1
  events += ["callback"]
  defer { cleaned += 1; events += ["defer"] }
  print f"observer:{value}"
  if value < 0 {
    return Err(GuardedCallbackFailure.Rejected(message: "observer failed"), cause: error.failure("observer cause")).context("guarded-observer", "guarded observer")
  }
  Ok(value + 1)
}

proc verify_typed_guarded_observables() [io, error] -> Result[Unit, Error] {
  let present: Int? = 7
  let absent: Int? = null
  assert guarded.typed_arithmetic(4) == 5
  assert guarded.typed_arithmetic(present) == 8
  assert guarded.typed_arithmetic(absent) == null
  assert guarded.typed_arithmetic(null) == null
  let present_path: Path? = p"/guarded/child"
  let missing_path: Path? = null
  assert guarded.typed_parent(p"/plain/child") == p"/plain"
  assert guarded.typed_parent(present_path) == p"/guarded"
  assert guarded.typed_parent(missing_path) == null
  assert guarded.typed_parent(null) == null
  let delayed = guarded.typed_capture(present)?
  assert (delayed |> collect()) == [8]
  assert (guarded.typed_capture(absent)? |> collect()) == [0]
  assert (guarded.typed_capture(null)? |> collect()) == [0]

  reset()
  assert guarded.typed_invoke(null, argument(4))? == null
  assert snapshot() == {events: ["argument"], calls: 0, cleaned: 0}
  reset()
  assert guarded.typed_invoke(observe, argument(4))? == 5
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
  let action: guarded.Observer? = observe
  reset()
  assert guarded.typed_invoke(action, argument(7))? == 8
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
  let no_action: guarded.Observer? = null
  reset()
  assert guarded.typed_invoke(no_action, argument(9))? == null
  assert snapshot() == {events: ["argument"], calls: 0, cleaned: 0}
  reset()
  let failure = guarded.typed_invoke(observe, argument(-1))
  match failure {
    Err(problem) => assert problem.message == "observer failed (guarded-observer: guarded observer)"
    Ok(_) => assert false
  }
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
}

proc verify_guarded_observables() [io, error] -> Result[Unit, Error] {
  verify_typed_guarded_observables()?
  let plain: Int = guarded.arithmetic(4)
  let missing: Null = guarded.arithmetic(null)
  let present: Int? = 7
  let absent: Int? = null
  let selected: Int? = guarded.arithmetic(present)
  let skipped: Int? = guarded.arithmetic(absent)
  assert plain == 5
  assert missing == null
  assert selected == 8
  assert skipped == null
  let plain_path: Path = guarded.parent(p"/plain/child")
  let missing_parent: Null = guarded.parent(null)
  let present_path: Path? = p"/guarded/child"
  let no_path: Path? = null
  assert plain_path == p"/plain"
  assert missing_parent == null
  assert guarded.parent(present_path) == p"/guarded"
  assert guarded.parent(no_path) == null
  let delayed = guarded.capture(present)?
  assert (delayed |> collect()) == [8]
  assert (guarded.capture(4)? |> collect()) == [5]
  assert (guarded.capture(absent)? |> collect()) == [0]
  assert (guarded.capture(null)? |> collect()) == [0]

  reset()
  let missing_result: Null = guarded.invoke(null, argument(4))?
  assert missing_result == null
  assert snapshot() == {events: ["argument"], calls: 0, cleaned: 0}
  reset()
  assert guarded.invoke(observe, argument(4))? == 5
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
  let action: guarded.Observer? = observe
  reset()
  assert guarded.invoke(action, argument(7))? == 8
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
  let no_action: guarded.Observer? = null
  reset()
  assert guarded.invoke(no_action, argument(9))? == null
  assert snapshot() == {events: ["argument"], calls: 0, cleaned: 0}
  reset()
  let failure = guarded.invoke(observe, argument(-1))
  match failure {
    Err(problem) => assert problem.message == "observer failed (guarded-observer: guarded observer)"
    Ok(_) => assert false
  }
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
}

proc propagate_guarded_failure() [io, error] -> Result[Unit, Error] {
  reset()
  defer { print snapshot().events.join(","); print snapshot().cleaned }
  let _ = guarded.invoke(observe, argument(-1))?
}

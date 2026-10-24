use closed-callback-provider as provider

type Snapshot = {events: List[Str], calls: Int, cleaned: Int}
error ClosedCallbackFailure = Rejected(message: Str)
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
    return Err(ClosedCallbackFailure.Rejected(message: "closed callback failed"), cause: error.failure("closed cause")).context("closed-observer", "closed callback")
  }
  Ok(value + 1)
}

proc verify_closed_callbacks() [io, error] -> Result[Unit, Error] {
  reset()
  assert provider.invoke(null, argument(4))? == null
  assert snapshot() == {events: ["argument"], calls: 0, cleaned: 0}
  let alias: provider.Observer = observe
  let action: provider.Observer? = alias
  reset()
  assert provider.invoke(action, argument(7))? == 8
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
  reset()
  assert provider.invoke(observe, argument(4))? == 5
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
  let no_action: provider.Observer? = null
  reset()
  assert provider.invoke(no_action, argument(9))? == null
  assert snapshot() == {events: ["argument"], calls: 0, cleaned: 0}
  reset()
  let failure = provider.invoke(observe, argument(-1))
  match failure {
    Err(ClosedCallbackFailure.Rejected {message}) => assert message == "closed callback failed"
    else => assert false
  }
  assert snapshot() == {events: ["argument", "callback", "defer"], calls: 1, cleaned: 1}
}

test closed_imported_callbacks { verify_closed_callbacks()? }

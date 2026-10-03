test test_wire_enum_nested_json_round_trip { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Seen = "seen", Missing = "", Unsupported = "unsupported" }
type Row = {state: State, history: List[State], optional: State?}
let raw = json.decode("{\"state\":\"seen\",\"history\":[\"\",\"unsupported\"],\"optional\":null}")?
let row = raw.require(Row)?
print (row.state == Seen)
print (row.history[0] == Missing)
print json.encode(row)?
print json.encode(Seen)?
print ("seen".require(State)? == Seen)
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """true
true
{"history":["","unsupported"],"optional":null,"state":"seen"}
"seen"
true
"""
}

test test_wire_enum_require_is_atomic_and_type_patterns_do_not_convert { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Seen = "seen", Missing = "missing" }
type Row = {states: List[State], state: State}
let raw = json.decode("{\"states\":[\"seen\",\"unknown\"],\"state\":\"seen\"}")?
match raw.require(Row) {
  Err(failure) => print $failure.message
  Ok(_) => print "unexpected success"
}
print json.encode(raw)?
let text: Any = "seen"
match text { _ is State => print "converted"; _ => print "raw string" }
let tag: Any = Seen
match tag { _ is State => print "actual enum"; _ => print "wrong tag" }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert "states[1]" in executed.stdout
  assert "{\"state\":\"seen\",\"states\":[\"seen\",\"unknown\"]}" in executed.stdout
  assert """raw string
actual enum
""" in executed.stdout
  {
    let assertion_condition = "unexpected success" not in executed.stdout
    let assertion_message = executed.stdout
    assert assertion_condition, assertion_message
  }
}

test test_wire_enum_rejects_ambient_coercion_and_invalid_declarations { |ctx|
  for source in [
    """enum State: Str { Seen = "seen" }
let state: State = "seen"
""",
    """enum State: Int { Seen = 1 }
""",
    """enum State: Str { Seen }
""",
    """enum State: Str { Seen = "same", Missing = "same" }
""",
    """enum State: Str { Seen(Int) = "seen" }
""",
    """enum State: Str { Seen = 1 }
""",
    """enum State: Str { Upper = "A", Lower = "a" }
let state: Str = Upper
""",
    """let spelling = "ready"
enum State: Str { Ready = spelling }
""",
  ] {
    let rejected = test.run_script(ctx, source)?
    {
      let assertion_condition = ! rejected.success
      let assertion_message = rejected.stderr
      assert assertion_condition, assertion_message
    }
  }
}

test test_wire_enum_import_identity_and_ordinary_enum_rejection { |ctx|
  let root = test.temp_dir(ctx, name: "wire-enum-imports")?
  fp"${root}/first.xsh".write_atomic("""##! First nominal state.
## Stable wire spelling.
export const spelling = "same"
## First state.
export enum State: Str { Seen = spelling }
## Prepared state.
export const prepared = Seen
""")?
  fp"${root}/second.xsh".write_atomic("""##! Second nominal state.
## Second state.
export enum State: Str { Seen = "same" }
""")?
  let executed = test.run_script(
    ctx,
    r"""use first as a
use second as b
let left: Any = a.Seen
let right: Any = b.Seen
print (left == right)
print json.encode(a.Seen)?
print json.encode(b.Seen)?
print (a.prepared == a.Seen)
print ("same".require(a.State)? == a.Seen)
match right { _ is a.State => print "wrong identity"; _ => print "different identity" }
match right.require(a.State) { Err(_) => print "require rejected"; Ok(_) => print "incorrectly accepted" }
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """false
"same"
"same"
true
true
different identity
require rejected
"""
  let rejected = test.run_script(
    ctx,
    """enum State { Seen }
print json.encode(Seen)?
""",
  )?
  {
    let assertion_condition = ! rejected.success
    let assertion_message = rejected.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.json-compatible" in rejected.stderr
  let dynamic = test.run_script(
    ctx,
    """enum State { Seen }
let value: Any = Seen
print json.encode(value)?
""",
  )?
  {
    let assertion_condition = ! dynamic.success
    let assertion_message = dynamic.stderr
    assert assertion_condition, assertion_message
  }
  assert "json-compatible" in dynamic.stderr
}

test test_wire_enum_map_values_and_missing_defaults { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Seen = "seen", Missing = "missing" }
type Row = {states: Map[State], note: Str = "default"}
let raw = json.decode("{\"states\":{\"a\":\"seen\",\"b\":\"missing\"},\"note\":\"supplied\"}")?
let row = raw.require(Row)?
print (row.states.get("a")? == Seen)
print json.encode(row)?
let missing = json.decode("{\"states\":{\"a\":\"seen\"}}")?
match missing.require(Row) { Err(_) => print "missing rejected"; Ok(_) => print "default incorrectly filled" }
let invalid = json.decode("{\"states\":{\"a\":\"unknown\"},\"note\":\"supplied\"}")?
match invalid.require(Row) { Err(failure) => print $failure.message; Ok(_) => print "unexpected success" }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert """true
{"note":"supplied","states":{"a":"seen","b":"missing"}}
missing rejected
""" in executed.stdout
  assert "states[\"a\"]" in executed.stdout
}

test test_wire_enum_constant_mappings_and_values { |ctx|
  let executed = test.run_script(
    ctx,
    r"""const prefix = "rea"
const ready_text = prefix + "dy"
enum State: Str { Ready = ready_text, Empty = "" }
const prepared = Ready
const packet = {state: prepared, states: [prepared, Empty]}
print json.encode(packet)?
print ("ready".require(State)? == prepared)
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """{"state":"ready","states":["ready",""]}
true
"""
  let duplicate = test.run_script(
    ctx,
    """const same = "same"
enum State: Str { First = same, Second = "same" }
""",
  )?
  {
    let assertion_condition = ! duplicate.success
    let assertion_message = duplicate.stderr
    assert assertion_condition, assertion_message
  }
  assert "check.enum-wire-mapping" in duplicate.stderr
}

test test_wire_enum_json_write_pretty_and_raw_decode { |ctx|
  let root = test.temp_dir(ctx, name: "wire-enum-json-write")?
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Ready = "ready", Empty = "" }
type Packet = {state: State, history: List[State]}
proc main(destination: Path) [fs, error] {
let packet = Packet(state: Ready, history: [Empty, Ready])
json.write(destination, packet, pretty: true)?
let raw = json.read(destination)?
match raw.state { text is Str => print $text; _ => print "incorrectly converted" }
let restored = raw.require(Packet)?
print (restored == packet)
print json.encode(restored)?
print ("\"ready\"" in json.encode(restored, pretty: true)?)
}
""",
    [fp"${root}/packet.json".display()],
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """ready
true
{"history":["","ready"],"state":"ready"}
true
"""
}

test test_wire_enum_static_import_and_dynamic_module_load_share_identity { |ctx|
  let root = test.temp_dir(ctx, name: "wire-enum-dynamic-identity")?
  let source = fp"${root}/state.xsh"
  source.write_atomic("""##! State identity fixture.
## External state.
export enum State: Str { Ready = "ready" }
## Prepared state.
export const prepared = Ready
""")?
  let executed = test.run_script(
    ctx,
    r"""use state as model
type Loaded = module { export let prepared: model.State }
proc main(source: Path) [fs, error] {
  let loaded = module.load(source)?.require(Loaded)?
  print (loaded.prepared == model.Ready)
  print json.encode(loaded.prepared)?
  match loaded.prepared { model.Ready => print "same constructor" }
}
""",
    [source.display()],
    {XSH_MODULE_PATH: root.display()},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """true
"ready"
same constructor
"""
}

test test_wire_enum_imported_generic_records_keep_declaring_mapping { |ctx|
  let root = test.temp_dir(ctx, name: "wire-enum-generic-records")?
  fp"${root}/state.xsh".write_atomic("""##! Generic state schema.
## Stable external spelling.
export const spelling = "ready"
## Declared state.
export enum State: Str { Ready = spelling, Empty = "" }
## Generic packet.
export type Packet[T] = {state: State, values: List[T], optional: State?}
## Concrete packet alias.
export type States = Packet[State]
""")?
  let executed = test.run_script(
    ctx,
    r"""use state as model
type Box[T] = {value: T}
type Nested = Box[model.States]
let raw = json.decode("{\"value\":{\"state\":\"ready\",\"values\":[\"\",\"ready\"],\"optional\":null}}")?
let packet = raw.require(Nested)?
print (packet.value.state == model.Ready)
print (packet.value.values[0] == model.Empty)
print json.encode(packet)?
let invalid = json.decode("{\"value\":{\"state\":\"ready\",\"values\":[\"\",\"unknown\"],\"optional\":null}}")?
match invalid.require(Nested) { Err(failure) => print $failure.message; Ok(_) => print "unexpected success" }
print json.encode(invalid)?
""",
    [],
    {XSH_MODULE_PATH: root.display()},
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert """true
true
{"value":{"optional":null,"state":"ready","values":["","ready"]}}
""" in executed.stdout
  assert "value.values[1]" in executed.stdout
  assert "{\"value\":{\"optional\":null,\"state\":\"ready\",\"values\":[\"\",\"unknown\"]}}" in executed.stdout
}

test test_wire_enum_strings_preserve_case_escapes_and_unicode { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Upper = "Ready", Lower = "ready", Escaped = "\0\\\"日本語" }
let values = [Upper, Lower, Escaped]
let encoded = json.encode(values)?
let decoded = json.decode(encoded)?
print (decoded.require(List[State])? == values)
print ("Ready".require(State)? == Upper)
print ("ready".require(State)? == Lower)
match "READY".require(State) { Err(_) => print "case rejected"; Ok(_) => print "unexpected success" }
print (json.encode(decoded.require(List[State])?)? == encoded)
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert executed.stdout == """true
true
true
case rejected
true
"""
}

test test_wire_enum_typed_map_values_preserve_key_domains { |ctx|
  let executed = test.run_script(
    ctx,
    r"""enum State: Str { Ready = "ready", Empty = "" }
const prepared: Map[Int, State] = {[1]: Ready, [2]: Empty}
print (prepared.get(1)? == Ready)
let source: Map[Int, Str] = {[1]: "ready", [2]: ""}
let trusted = source.require(Map[UInt, State])?
print (trusted.get(1)? == Ready)
print json.encode(trusted.values())?
let dynamic: Any = trusted
match json.encode(dynamic) { Err(_) => print "non-Str JSON keys rejected"; Ok(_) => print "unexpected JSON success" }
let raw = json.decode("{\"1\":\"ready\"}")?
match raw.require(Map[UInt, State]) { Err(_) => print "raw keys rejected"; Ok(_) => print "unexpected key conversion" }
let negative: Map[Int, Str] = {[-1]: "ready"}
match negative.require(Map[UInt, State]) { Err(failure) => print $failure.message; Ok(_) => print "unexpected UInt success" }
let invalid: Map[Int, Str] = {[1]: "ready", [2]: "unknown"}
match invalid.require(Map[Int, State]) { Err(failure) => print $failure.message; Ok(_) => print "unexpected wire success" }
print (invalid.get(1)? == "ready")
let flags: Map[Bool, Str] = {[false]: "ready", [true]: ""}
let converted = flags.require(Map[Bool, State])?
print (converted.get(false)? == Ready)
let actual: Any = source
match actual { _ is Map[UInt, State] => print "unexpected type conversion"; _ => print "actual values checked" }
""",
  )?
  {
    let {success: assertion_condition, stderr: assertion_message, ..} = executed
    assert assertion_condition, assertion_message
  }
  assert """true
true
["ready",""]
non-Str JSON keys rejected
raw keys rejected
""" in executed.stdout
  assert "UInt key" in executed.stdout
  assert "unknown wire string" in executed.stdout
  assert "$[2]" in executed.stdout
  assert """true
true
actual values checked
""" in executed.stdout
}

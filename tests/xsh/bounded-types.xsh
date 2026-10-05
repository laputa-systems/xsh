# `type Port = Int range 1..=65535` is an `Int` known to lie between two
# constant bounds. A value gets the type from an integer literal the checker
# can judge or from one validation at an explicit boundary; every operation
# reads it as the integer it is and returns what an integer returns.

type Port = Int range 1..=65535

# `..` stops before its upper bound, as a slice does: this is 0 to 255.
type Byte = UInt range 0..256

type Offset = Int range -5..=5

type Chunk = UInt range 1KiB..=1MiB

type Server = {host: Str, port: Port, backlog: Byte}

const HTTPS: Port = 443

pure widen(port: Port) -> Int {
  port
}

pure pick(first: Port, second: Port?) -> Port {
  second ?? first
}

pure parse(text: Str) -> Result[Port] {
  let number = text as Int
  Ok(number as Port)
}

proc echo_port(port: Port) [error] -> Result[Str] {
  Ok(f"{port}")
}

test test_a_literal_in_range_has_the_bounded_type {
  let port: Port = 8080
  let low: Port = 1
  let high: Port = 65535
  let byte: Byte = 255
  let negative: Offset = -5
  let chunk: Chunk = 64KiB
  assert widen(port) == 8080
  assert widen(low) + widen(high) == 65536
  assert widen(443) == 443
  assert byte == 255 and negative == -5 and chunk == 65536
  assert pick(port, null) == 8080
  assert pick(port, 80) == 80
  assert HTTPS == 443

  let ports: List[Port] = [80, HTTPS, port]
  let maybe: Port? = 22
  let server = Server(host: "localhost", port: 8443, backlog: 16)
  let built = Server(host: "example", port:, backlog: 0)
  assert ports.len() == 3 and maybe == 22
  assert server.port == 8443 and built.backlog == 0
}

test test_a_bounded_value_fits_its_base_at_any_depth {
  let port: Port = 8080
  let number: Int = port
  let byte: Byte = 7
  let size: UInt = byte
  let ports: List[Port] = [80, 443]
  let numbers: List[Int] = ports
  let maybe: Port? = port
  let wide: Int? = maybe
  let dynamic: Any = port
  assert number == 8080 and size == 7
  assert numbers == [80, 443]
  assert wide == 8080
  assert dynamic is Int
}

# Each of these reads an integer through a path lowering specialises for
# `Int`: the bounded value takes the same path as the integer it is stored as.
test test_integer_operations_read_a_bounded_value_as_an_integer {
  let port: Port = 8080
  let byte: Byte = 200
  let index: Offset = 1
  let other: Port = 443

  # Arithmetic returns the base type.
  let widened = port + other
  let doubled = byte * 2
  assert widened == 8523 and doubled == 400
  assert port - 80 == 8000 and port / 2 == 4040 and port % 100 == 80
  assert -index == -1
  var total = 0
  total += port
  total -= other
  assert total == 7637

  # Comparison, against a bounded value, an integer, and a literal outside
  # the bounds.
  assert other < port
  assert port >= other
  assert port > 100 and port < 70000 and 0 < port
  assert 1 < other < port <= 8080
  assert port == 8080 and port != other
  assert port == widened - 443

  # Indexing, slicing, and `range`.
  let letters = ["a", "b", "c"]
  assert letters[index] == "b"
  assert letters[index..3] == ["b", "c"]
  assert "abcdef"[index..byte - 196] == "bcd"
  var steps = 0
  for step in range(index + 2) {
    steps += step
  }

  assert steps == 3
  let visited = [item + 1 for item in [index, index]]
  assert visited == [2, 2]

  # Formatting, with and without a format spec.
  assert f"{port}" == "8080"
  assert f"{port:>6}|{byte:<5}|{index:03}" == "  8080|200  |001"

  # A command argument, a JSON value, and a match subject.
  let argv = run.text printf "%s:%s" $port $byte
  assert argv == "8080:200"
  assert json.encode({port: port, sizes: [byte]})? == "{\"port\":8080,\"sizes\":[200]}"
  let name = match other {
    443 => "https",
    80 => "http",
    else => "other",
  }
  assert name == "https"

  # Membership, sorting, and a method of the base type.
  let ports: List[Port] = [8080, 443]
  assert other in ports and port in [1, 8080]
  let sorted = ports |> sort |> collect()
  assert sorted[0] == 443
  assert port.float() == 8080.0
  assert echo_port(port)? == "8080"
}

test test_validation_runs_once_at_the_explicit_boundary {
  let number = 8080
  let port = number.require(Port)?
  assert widen(port) == 8080
  assert widen(number as Port) == 8080
  assert parse("443")? == 443

  for outside in [0, -1, 65536] {
    match outside.require(Port) {
      Ok(_) => assert false, "an integer outside the range was validated"
      Err(error) => assert f"expected Int range 1..=65535, found {outside}" in error.message, error.message
    }

    let converted = try { outside as Port }
    assert converted is Err(_)
  }

  assert parse("70000") is Err(_)
  assert parse("x") is Err(_)

  # The half-open form excludes its upper bound.
  assert 255.require(Byte) is Ok(_)
  assert 256.require(Byte) is Err(_)
  assert (-1).require(Byte) is Err(_)

  # A type test narrows, and a dynamic value is tested the same way.
  assert number is Port
  if number is Port {
    assert widen(number) == 8080
  }

  let dynamic: Any = 70000
  assert ! (dynamic is Port)
  assert dynamic.require(Port) is Err(_)
  let text: Any = "80"
  assert text.require(Port) is Err(_)

  # Arithmetic leaves the bounds; the result is validated again to return.
  let next = (port + 1).require(Port)?
  assert next == 8081
}

test test_bounded_slots_in_schemas_validate {
  let server = json.decode("{\"host\": \"a\", \"port\": 8443, \"backlog\": 5}")?.require(Server)?
  assert server.port == 8443 and widen(server.port) == 8443

  match json.decode("{\"host\": \"a\", \"port\": 0, \"backlog\": 5}")?.require(Server) {
    Ok(_) => assert false, "a port outside the range was validated"
    Err(error) => assert "schema check failed at port: expected Int range 1..=65535, found 0" in error.message, error.message
  }

  match json.decode("{\"host\": \"a\", \"port\": 1, \"backlog\": 256}")?.require(Server) {
    Ok(_) => assert false, "a backlog outside the range was validated"
    Err(error) => assert "backlog: expected UInt range 0..=255, found 256" in error.message, error.message
  }

  let ports = json.decode("[80, 443]")?.require(List[Port])?
  assert ports.len() == 2
  assert json.decode("[80, 0]")?.require(List[Port]) is Err(_)
}

# A dynamic call passes an unchecked value to a checked parameter, so the
# parameter's bounds are tested where the call arrives.
test test_dynamic_call_tests_the_bounds_at_the_parameter { |ctx|
  let callee = echo_port
  assert callee.call(80)? == "80"

  let output = test.run_script(
    ctx,
    r"""type Port = Int range 1..=65535

proc echo_port(port: Port) [error] -> Result[Str] {
  Ok(f"{port}")
}

let callee: Proc = echo_port
let result = callee.call(70000)
print ${result is Err(_)}
""",
  )?
  assert output.status == 3, f"{output.stdout}{output.stderr}"
  assert "expected Port, found Int" in output.stderr, output.stderr
}

proc check_errors(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

test test_a_literal_outside_the_range_is_rejected { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Port = Int range 1..=65535
type Byte = UInt range 0..256
type Server = {host: Str, port: Port}

pure listen(port: Port = 0) -> Port {
  port
}

pure fixed() -> Port {
  65536
}

let low: Port = 0
let byte: Byte = 256
let negative: Byte = -1
let ports: List[Port] = [80, 70000]
let maybe: Port? = 0
let server = Server(host: "a", port: 0)
let literal: Server = {host: "a", port: 65536}
var current: Port = 1
current = 0
print ${listen(0)} ${fixed()} $low $byte $negative ${ports.len()} ${maybe ?? 1} ${server.port} ${literal.port} $current
""",
  )?
  assert count(stderr, "err[check.validated-literal]") == 11, stderr
  assert count(stderr, "err[") == 11, stderr
  assert "0 is outside the range of `Int range 1..=65535`" in stderr, stderr
  assert "256 is outside the range of `UInt range 0..=255`" in stderr, stderr
  assert "-1 is outside the range of `UInt range 0..=255`" in stderr, stderr
  assert "write an integer from 1 to 65535" in stderr, stderr
}

test test_an_unvalidated_integer_never_fits_a_bounded_type { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Port = Int range 1..=65535
type Low = Int range 1..=1023
type Byte = UInt range 0..256

pure listen(port: Port) -> Port {
  port
}

let number = 80
let size: UInt = 80
let low: Low = 80
let plain: Port = number
let unsigned: Port = size
let other: Port = low
let sum: Port = listen(80) + 1
let numbers: List[Int] = [80]
let ports: List[Port] = numbers
let byte: Byte = listen(80)
print ${listen(number)} $plain $unsigned $other $sum ${ports.len()} $byte
""",
  )?
  assert count(stderr, "err[check.type-mismatch]") == 7, stderr
  assert count(stderr, "err[") == 7, stderr
  assert "expected Int range 1..=65535, found Int" in stderr, stderr
  assert "expected Int range 1..=65535, found UInt" in stderr, stderr
  assert "expected Int range 1..=65535, found Int range 1..=1023" in stderr, stderr
  assert "validate it with `.require(...)?` or `as`" in stderr, stderr
  assert "arithmetic on a bounded value returns its base type" in stderr, stderr
}

test test_a_bounded_variable_is_not_counted_with { |ctx|
  let stderr = check_errors(
    ctx,
    r"""type Port = Int range 1..=65535

var port: Port = 80
port += 1
print $port
""",
  )?
  assert count(stderr, "err[") == 1, stderr
  assert "err[check.operator-type]: `+=` is not defined for Int range 1..=65535" in stderr, stderr
  assert "assign the validated result" in stderr, stderr
}

test test_a_constant_outside_the_range_is_rejected { |ctx|
  let stderr = check_errors(ctx, "type Port = Int range 1..=65535\n\nconst NONE: Port = 0\nprint $NONE\n")?
  assert "err[check.validated-literal]: 0 is outside the range of `Int range 1..=65535`" in stderr, stderr
  assert count(stderr, "err[") == 1, stderr
}

test test_a_range_is_declared_over_an_integer_with_bounds_that_hold_a_value { |ctx|
  let rejected = [
    {
      source: "type Empty = Int range 5..5\n",
      message: "the range of `type Empty` holds no value: 5..5 is empty",
    },
    {
      source: "type Backwards = Int range 9..=1\n",
      message: "the range of `type Backwards` holds no value: 9..=1 is empty",
    },
    {
      source: "type Text = Str range 1..=2\n",
      message: "`type Text`: a range bounds an Int or a UInt, not Str",
    },
    {
      source: "type Ratio = Float range 0..=1\n",
      message: "`type Ratio`: a range bounds an Int or a UInt, not Float",
    },
    {
      source: "type Signed = UInt range -1..=5\n",
      message: "a UInt is never negative",
    },
    {
      source: "type Huge = Int range 0..=99999999999999999999\n",
      message: "a bound of `type Huge` is outside the 64-bit signed range",
    },
    {
      source: "type Port = Int range 1..=65535\ntype Low = Port range 1..=1023\n",
      message: "`type Low`: a range bounds an Int or a UInt, not Int range 1..=65535",
    },
    {
      source: "type Sized[T] = Int range 1..=2\n",
      message: "`type Sized` has a range, so it takes no type parameters",
    },
  ]
  for case in rejected {
    let stderr = check_errors(ctx, case.source)?
    assert "err[check.schema]" in stderr, stderr
    assert case.message in stderr, stderr
  }

  for source in [
    "type Port = Int range 1.. =5\n",
    "type Port = Int range 1...5\n",
    "type Port = Int range low..=5\n",
    "type Port = Int range 1..=\n",
    "type Port = {port: Int} range 1..=5\n",
    "let port: Int range 1..=5 = 2\n",
  ] {
    let stderr = check_errors(ctx, source)?
    assert "err[parse." in stderr, stderr
  }
}

# A map key and a set element are one of the ordered scalar types themselves:
# the collection orders and compares what it stores, and stores the base. A
# bounded value fits the base, so it keys a `Map[Int, V]` and is an element
# of a `Set[Int]`.
test test_a_bounded_type_is_not_a_map_key_or_a_set_element { |ctx|
  let stderr = check_errors(ctx, "type Port = Int range 1..=65535\n\ntype Names = Map[Port, Str]\n")?
  assert "err[check.map-key-type]" in stderr, stderr
  let elements = check_errors(ctx, "type Port = Int range 1..=65535\n\ntype Open = Set[Port]\n")?
  assert "err[check.set-element-type]" in elements, elements

  let port: Port = 443
  let open: Set[Int] = {80, port}
  assert port in open and open.len() == 2
}

# A nominal type is a record schema, so it has no range.
test test_a_nominal_type_has_no_range { |ctx|
  let stderr = check_errors(ctx, "nominal type Port = Int range 1..=10\n")?
  assert "err[check.schema]" in stderr, stderr
  assert "`nominal type Port` must be a record schema" in stderr, stderr
}

test test_range_stays_a_name_outside_a_type_declaration {
  let range = 3
  var total = 0
  for step in range(range) {
    total += step
  }

  assert total == 3
}

test test_fmt_writes_the_bounds_as_declared { |ctx|
  let source = "type   Port = Int   range   1 ..= 65535\nexport type Byte = UInt range 0..256\ntype Offset = Int range -5..=5\ntype Chunk = UInt range 1KiB..=1MiB\n"
  let file = test.temp_file(ctx, name: "bounded.xsh", contents: bytes.from_text(source))?
  let formatted = run.capture --text "xsht" fmt $file
  assert formatted.status.exited_with(0), formatted.stderr
  assert file.read_text()? == "type Port = Int range 1..=65535\n\nexport type Byte = UInt range 0..256\n\ntype Offset = Int range -5..=5\n\ntype Chunk = UInt range 1KiB..=1MiB\n"
  let again = run.capture --text "xsht" fmt --check $file
  assert again.status.exited_with(0), again.stderr
}

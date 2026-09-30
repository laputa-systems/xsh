test generic_row_projection_shapes_and_field_types [error] { |ctx|
  let output = test.run_script(ctx, r"""pure name(entry) { entry.name }
let narrow = {name: "narrow"}
let wide_first = {name: "wide-first", tag: 11, active: true}
let wide_last = {active: false, tag: 23, name: "wide-last"}
let number = {tag: "number", name: 17}
let flag = {name: false, tag: "flag"}
print ${name(narrow)} ${name(wide_first)} ${name(wide_last)} ${name(number)} ${name(flag)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "narrow wide-first wide-last 17 false\n"
}

test generic_row_projection_forwarded [error] { |ctx|
  let output = test.run_script(ctx, r"""pure name(entry) { entry.name }
pure forwarded(entry) { name(entry) }
print ${forwarded({name: "small"})} ${forwarded({tag: 9, name: "large"})} ${forwarded({name: 31, active: true})}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "small large 31\n"
}

test generic_row_projection_nested_layouts [error] { |ctx|
  let output = test.run_script(ctx, r"""pure nested_name(entry) { entry.person.name }
let narrow = {person: {name: "nested"}}
let wide_first = {person: {name: "first", age: 7}, tag: "outer-first"}
let wide_last = {tag: "outer-last", person: {age: 11, name: "last"}}
let number = {tag: false, person: {age: 13, name: 41}}
print ${nested_name(narrow)} ${nested_name(wide_first)} ${nested_name(wide_last)} ${nested_name(number)}
""")?
  assert output.success, output.stderr
  output.status == 0
  output.stdout == "nested first last 41\n"
}

test generic_row_projection_missing_field_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure name(entry) { entry.name }
let _ = name({age: 7})
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "field" in output.stderr, output.stderr
  assert "name" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_row_projection_nested_missing_field_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure nested_name(entry) { entry.person.name }
let _ = nested_name({person: {age: 7}})
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "field" in output.stderr, output.stderr
  assert "name" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_row_projection_wrong_field_type_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure string_name(entry) {
  let value: Str = entry.name
  value
}
let _ = string_name({name: 17})
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "Str" in output.stderr, output.stderr
  assert "Int" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

test generic_row_recursive_infinite_equation_rejected [error] { |ctx|
  let output = test.run_script(ctx, r"""pure infinite(entry) -> Int {
  if false { infinite(entry.name) } else { 0 }
}
let _ = infinite({name: 7})
""")?
  output.status == 2
  output.stdout == ""
  assert "check." in output.stderr, output.stderr
  assert "infinite type" in output.stderr or "occurs" in output.stderr or "recursive type" in output.stderr, output.stderr
  assert "parse." not in output.stderr, output.stderr
}

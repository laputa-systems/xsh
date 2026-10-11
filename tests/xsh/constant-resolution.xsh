test test_with_initializers_read_the_previous_binding_instead_of_a_constant { |ctx|
  test.expect(
    ctx,
    r"""const item = 1
with item = Ok(2), copied = Ok(item) {
  assert copied == 2
} else {
  assert false
}
assert item == 1
""",
    status: 0,
  )?
}

test test_with_initializers_keep_the_type_of_the_previous_binding { |ctx|
  test.expect(
    ctx,
    r"""const item = 1
with item = Ok("runtime"), copied = Ok(item) {
  assert copied.upper() == "RUNTIME"
} else {
  assert false
}
""",
    status: 0,
  )?
}

test test_resource_initializers_resolve_preceding_handles { |ctx|
  test.expect(
    ctx,
    r"""const root = /not-a-resource
with root = fs.tempdir()?, view = fs.open_root(root.host_path()?)? {
  assert view.exists(p".")?
}
""",
    status: 0,
  )?
}

test test_nested_with_bindings_restore_each_enclosing_identity { |ctx|
  test.expect(
    ctx,
    r"""const item = "outer"
with item = Ok("middle"), copied = Ok(item) {
  with item = Ok("inner"), leaf = Ok(item) {
    assert leaf == "inner"
  } else {
    assert false
  }
  assert item == "middle"
  assert copied == "middle"
} else {
  assert false
}
assert item == "outer"
""",
    status: 0,
  )?
}

test test_runtime_bindings_shadow_constant_enum_constructors { |ctx|
  test.expect(
    ctx,
    r"""enum State { Ready }
const fallback = Ready
with Ready = Ok(9), copied = Ok(Ready) {
  assert copied == 9
} else {
  assert false
}
""",
    status: 0,
  )?
}

test test_bare_enum_patterns_keep_the_constructor_visible_to_constants { |ctx|
  test.expect(
    ctx,
    r"""enum State { ready }
const selected = ready
match selected {
  ready => {
    const copied = ready
    assert copied == selected
  }
}
""",
    status: 0,
  )?
}

test test_pattern_alternatives_publish_one_capture_identity { |ctx|
  test.expect(
    ctx,
    r"""enum Event { Left(Int), Right(Int) }
match Left(3) {
  Left(value) | Right(value) => { assert value == 3 }
}
""",
    status: 0,
  )?
}

test test_global_imports_resolve_in_forward_constant_initializers { |ctx|
  let root = test.temp_dir(ctx, name: "forward-constant-module")?
  fp"{root}/config.xsh".write_atomic(r"""##! Forward constant data.
## A prepared size.
export const size = 3
""")
  let output = test.run_script(
    ctx,
    r"""const count = c.size + 1
use config as c
assert count == 4
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert output.success, output.stderr
}

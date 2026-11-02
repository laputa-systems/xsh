test test_resource_initializers_resolve_preceding_handles { |ctx|
  test.expect(
    ctx,
    r"""const root = p"/not-a-resource"
with root = fs.tempdir()?, view = fs.open_root(root.host_path()?)? {
  assert view.exists(p".")?
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
  test.expect(
    ctx,
    r"""const count = c.size + 1
use config as c
assert count == 4
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: root},
  )?
}

# Command splices have a source identity even though they have no expression row.
test test_command_splices_keep_each_checked_binding { |ctx|
  let output = test.expect(
    ctx,
    r"""proc emit(words = ["parameter"]) { run printf "%s\n" @words }
const words = ["constant"]
run printf "%s\n" @words
if true {
  let words = ["runtime"]
  run printf "%s\n" @words
}
run printf "%s\n" @words
emit()
""",
    status: 0,
  )?
  assert output.stdout == "constant\nruntime\nconstant\nparameter\n"
}

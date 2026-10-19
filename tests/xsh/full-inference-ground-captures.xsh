test test_full_inference_ground_result_reader_observes_mutable_shared_updates { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""var shared = 1
proc observe() [] -> Result[Int] { Ok(shared) }
proc set_shared(value: Int) [] -> Unit { shared = value }
let before = observe()?
set_shared(2)
let after_proc = observe()?
shared = 3
let after_root = observe()?
assert before == 1
assert after_proc == 2
assert after_root == 3
print f"{before} {after_proc} {after_root}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "1 2 3\n"
}

test test_full_inference_mutable_callable_record_keeps_ground_handle_and_any_sibling { |ctx|
  let output = test.run_xsh(
    ctx,
    r"""type Holder = {handle: Pure, ignored: Any}
pure first() -> Int { 11 }
pure second() -> Int { 22 }
var holder: Holder = {handle: first, ignored: "start"}
proc replace_fields() [] -> Unit {
  holder.handle = second
  holder.ignored = "field"
}
pure read_handle() -> Any { holder.handle.call() }
proc read_ignored() [error] -> Result[Str] {
  holder.ignored.require(Str)?
}
let before = read_handle().require(Int)?
replace_fields()
let after_field_write = read_handle().require(Int)?
let field_data = read_ignored()?
holder = {handle: first, ignored: "whole"}
let after_whole_write = read_handle().require(Int)?
let whole_data = read_ignored()?
assert before == 11
assert after_field_write == 22
assert field_data == "field"
assert after_whole_write == 11
assert whole_data == "whole"
print f"{before} {after_field_write} {field_data} {after_whole_write} {whole_data}"
""",
  )?
  assert output.success, output.stderr
  assert output.stdout == "11 22 field 11 whole\n"
}

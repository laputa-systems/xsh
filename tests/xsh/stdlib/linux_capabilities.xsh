pure errno_of(result: Result[Any, Error]) -> Int {
  if let Err(failure) = result {
    return failure.errno ?? -1
  } else {
    return 0
  }
}

# The privilege setters change per-thread kernel state that children inherit,
# so each test that mutates it runs the change in its own xsh process. The
# prelude gives those scripts the same errno helper.
const PRELUDE = """pure errno_of(result: Result[Any, Error]) -> Int {
  if let Err(failure) = result {
    return failure.errno ?? -1
  } else {
    return 0
  }
}
"""

pure hex_value(text: Str) -> Int {
  var value = 0
  for index in range(text.byte_len()) {
    value = value * 16 + ("0123456789abcdef".find(text.byte_slice(index, 1)) ?? 0)
  }
  value
}

pure bits_of(mask: Int) -> List[Int] {
  var rest = mask
  var found: List[Int] = []
  var number = 0
  while rest > 0 {
    if rest % 2 == 1 { found += [number] }
    rest = rest / 2
    number += 1
  }
  found
}

test test_privileges_report_sorted_capability_numbers_and_the_kernel_limit { |ctx|
  let state = linux.privileges()?
  assert state.last_capability >= 0 and state.last_capability < 64
  for numbers in [state.effective, state.permitted, state.inheritable, state.bounding, state.ambient] {
    var previous = -1
    for number in numbers {
      assert number > previous and number <= state.last_capability
      previous = number
    }
  }
  # The kernel clears effective capabilities that are not permitted.
  for number in state.effective { assert number in state.permitted }
  for number in state.ambient { assert number in state.permitted and number in state.inheritable }
  assert state.parent_death_signal >= 0
}

test test_privileges_match_the_masks_in_proc_status { |ctx|
  let state = linux.privileges()?
  var lists: Map[Str, List[Int]] = {}
  for line in p"/proc/self/status".read_text()?.lines() {
    for label in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
      if line.starts_with(label + ":") {
        lists[label] = bits_of(hex_value(line.byte_slice(label.byte_len() + 1).trim()))
      }
    }
  }
  assert lists["CapInh"] == state.inheritable
  assert lists["CapPrm"] == state.permitted
  assert lists["CapEff"] == state.effective
  assert lists["CapBnd"] == state.bounding
  assert lists["CapAmb"] == state.ambient
}

test test_set_no_new_privs_is_visible_and_inherited { |ctx|
  let child = test.temp_file(ctx, name: "nnp-child.xsh", contents: b"print f\"{linux.privileges()?.no_new_privs}\"\n")?
  let result = test.expect(ctx, PRELUDE + r"""print f"{linux.privileges()?.no_new_privs}"
linux.set_no_new_privs()?
print f"{linux.privileges()?.no_new_privs}"
unix.exec(process.command_argv(args[0], [args[0], args[1]]))?
""", status: 0, args: [ctx.xsh_bin, child])?
  assert result.stdout == "false\ntrue\ntrue\n"
}

test test_parent_death_signal_is_set_cleared_and_validated { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""linux.set_parent_death_signal(15)?
print f"{linux.privileges()?.parent_death_signal}"
linux.set_parent_death_signal(0)?
print f"{linux.privileges()?.parent_death_signal}"
print f"{errno_of(linux.set_parent_death_signal(1000))}"
""", status: 0)?
  assert result.stdout == "15\n0\n22\n"
  test.error_kind(linux.set_parent_death_signal(-1), "invalid-argument")
}

test test_parent_death_signal_survives_exec { |ctx|
  let child = test.temp_file(ctx, name: "pdeath-child.xsh", contents: b"print f\"{linux.privileges()?.parent_death_signal}\"\n")?
  let result = test.expect(ctx, PRELUDE + r"""linux.set_parent_death_signal(10)?
unix.exec(process.command_argv(args[0], [args[0], args[1]]))?
""", status: 0, args: [ctx.xsh_bin, child])?
  assert result.stdout == "10\n"
}

test test_capability_numbers_are_validated_before_the_kernel { |ctx|
  test.error_kind(linux.drop_bounding_capability(64), "invalid-argument")
  test.error_kind(linux.drop_bounding_capability(-1), "invalid-argument")
  test.error_kind(linux.set_ambient_capability(64, false), "invalid-argument")
  test.error_kind(linux.set_capabilities(effective: [], permitted: [], inheritable: [64]), "invalid-argument")
  test.error_kind(linux.set_securebits(["no-such-flag"]), "invalid-argument")
  test.error_kind(linux.set_ptracer(-2), "invalid-argument")
}

test test_inheritable_set_can_always_be_emptied { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""let before = linux.privileges()?
linux.set_capabilities(effective: before.effective, permitted: before.permitted, inheritable: [])?
let after = linux.privileges()?
print f"{after.inheritable.len()} {after.effective == before.effective} {after.permitted == before.permitted}"
""", status: 0)?
  assert result.stdout == "0 true true\n"
}

test test_set_capabilities_reports_the_kernel_refusal_for_growth { |ctx|
  # No process may gain a permitted capability it does not have.
  let result = test.expect(ctx, PRELUDE + r"""let before = linux.privileges()?
var missing = -1
for number in range(before.last_capability + 1) {
  if ! (number in before.permitted) and missing < 0 { missing = number }
}
if missing < 0 {
  print "skip"
} else {
  let outcome = linux.set_capabilities(effective: before.effective, permitted: before.permitted + [missing], inheritable: before.inheritable)
  print f"{errno_of(outcome)}"
}
""", status: 0)?
  assert result.stdout in ["skip\n", "1\n"]
}

test test_bounding_drop_needs_setpcap_and_is_permanent { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""let before = linux.privileges()?
let allowed = 8 in before.effective
if before.bounding.is_empty() {
  print "skip"
} else {
  let target = before.bounding[before.bounding.len() - 1]
  let errno = errno_of(linux.drop_bounding_capability(target))
  print f"{allowed} {errno} {target in linux.privileges()?.bounding}"
}
""", status: 0)?
  assert result.stdout in ["skip\n", "true 0 false\n", "false 1 true\n"]
}

test test_unknown_capability_number_is_refused_by_the_kernel { |ctx|
  # The kernel checks CAP_SETPCAP before it validates the number.
  let expected = if 8 in linux.privileges()?.effective { 22 } else { 1 }
  assert errno_of(linux.drop_bounding_capability(63)) == expected
}

test test_securebits_round_trip_when_setpcap_is_held { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""let allowed = 8 in linux.privileges()?.effective
let errno = errno_of(linux.set_securebits(["noroot", "no_setuid_fixup"]))
print f"{allowed} {errno} {json.encode(linux.privileges()?.securebits)?}"
""", status: 0)?
  assert result.stdout in ["true 0 [\"noroot\",\"no_setuid_fixup\"]\n", "false 1 []\n"]
}

test test_locked_securebits_cannot_be_cleared_again { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""let allowed = 8 in linux.privileges()?.effective
let first = errno_of(linux.set_securebits(["noroot_locked", "noroot"]))
let second = errno_of(linux.set_securebits([]))
print f"{allowed} {first} {second} {json.encode(linux.privileges()?.securebits)?}"
""", status: 0)?
  assert result.stdout in ["true 0 1 [\"noroot\",\"noroot_locked\"]\n", "false 1 1 []\n"]
}

test test_keep_capabilities_flag_is_a_securebit_and_clears_on_request { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""linux.set_keep_capabilities(true)?
print json.encode(linux.privileges()?.securebits)?
linux.set_keep_capabilities(false)?
print json.encode(linux.privileges()?.securebits)?
""", status: 0)?
  assert result.stdout == "[\"keep_caps\"]\n[]\n"
}

test test_ambient_raise_needs_the_capability_to_be_inheritable { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""let before = linux.privileges()?
var candidate = -1
for number in range(before.last_capability + 1) {
  if number in before.permitted and ! (number in before.inheritable) and candidate < 0 { candidate = number }
}
if candidate < 0 {
  print "skip"
} else {
  print f"{errno_of(linux.set_ambient_capability(candidate, true))} {errno_of(linux.set_ambient_capability(candidate, false))}"
}
""", status: 0)?
  assert result.stdout in ["skip\n", "1 0\n"]
}

test test_ambient_raise_follows_the_inheritable_set { |ctx|
  let result = test.expect(ctx, PRELUDE + r"""let before = linux.privileges()?
var candidate = -1
for number in range(before.last_capability + 1) {
  if number in before.permitted and number in before.bounding and candidate < 0 { candidate = number }
}
if candidate < 0 {
  print "skip"
} else {
  let inheritable = if candidate in before.inheritable { before.inheritable } else { before.inheritable + [candidate] }
  let widened = errno_of(linux.set_capabilities(effective: before.effective, permitted: before.permitted, inheritable: inheritable))
  let raised = errno_of(linux.set_ambient_capability(candidate, true))
  let on = candidate in linux.privileges()?.ambient
  let lowered = errno_of(linux.set_ambient_capability(candidate, false))
  let off = candidate in linux.privileges()?.ambient
  print f"{widened} {raised} {on} {lowered} {off}"
}
""", status: 0)?
  # A permitted capability may always enter the inheritable set, so the whole
  # sequence succeeds for any capability the process holds.
  assert result.stdout in ["skip\n", "0 0 true 0 false\n"]
}

test test_ptracer_accepts_none_any_and_a_pid_or_reports_einval { |ctx|
  # Without the Yama security module the kernel rejects PR_SET_PTRACER.
  for pid in [0, -1, process.current_pid()?] {
    let outcome = linux.set_ptracer(pid)
    assert outcome is Ok(_) or errno_of(outcome) == 22
  }
}

test test_set_resuid_and_resgid_validate_and_accept_current_identity { |ctx|
  let me = unix.id()?
  test.error_kind(unix.set_resuid(real: 4294967295, effective: null, saved: null), "unix-set-resuid")
  test.error_kind(unix.set_resgid(real: null, effective: -1, saved: null), "unix-set-resgid")
  assert unix.set_resuid(real: null, effective: null, saved: null) is Ok(_)
  assert unix.set_resuid(real: me.uid, effective: me.euid, saved: me.euid) is Ok(_)
  assert unix.set_resgid(real: me.gid, effective: me.egid, saved: me.egid) is Ok(_)
}

test test_set_resuid_reports_the_kernel_refusal_without_privilege { |ctx|
  let me = unix.id()?
  # CAP_SETUID and CAP_SETGID let any ID through.
  if 7 in linux.privileges()?.effective or 6 in linux.privileges()?.effective { test.skip("process holds CAP_SETUID or CAP_SETGID") }
  let other = if me.uid == 4321 { 4322 } else { 4321 }
  assert errno_of(unix.set_resuid(real: other, effective: null, saved: null)) == 1
  assert errno_of(unix.set_resgid(real: other, effective: null, saved: null)) == 1
}

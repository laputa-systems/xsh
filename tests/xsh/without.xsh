# `without EFFECT, ... { BODY }` is a lexical block the checker holds to the
# enclosing effect bound minus the listed effects. It changes nothing at run
# time.

proc record(log: Path, line: Str) [fs, error] {
  log.write(f"{log.read_text() ?? ""}{line}\n")
}

proc offline_steps(log: Path) [fs, net, error] {
  record(log, "before")
  without net {
    defer record(log, "cleanup")?

    record(log, "inside")
  }

  record(log, "after")
}

# The region is a scope: its bindings end with it, its defers run when it
# ends, and it may assign to bindings outside it.
test test_without_runs_as_a_lexical_block { |ctx|
  let log = test.temp_file(ctx, name: "without.log")?
  offline_steps(log)
  assert log.read_text()? == "before\ninside\ncleanup\nafter\n"

  var total = 0
  without net, process {
    let step = 2
    total += step
    without fs {
      total += step
    }
  }

  assert total == 4
}

# `without` is a word only in the statement head.
test test_without_stays_an_ordinary_name {
  let without = [1, 2]
  let options = {without: without, net: 3}
  assert without.len() == 2
  assert options.without.len() + options.net == 5
}

proc violations(ctx: TestContext, source: Str) [fs, process, env, error] -> Result[Str] {
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert output.stdout == "", output.stdout
  output.stderr
}

pure count(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

test test_without_rejects_a_host_operation_in_the_region { |ctx|
  let stderr = violations(
    ctx,
    r"""proc load(file: Path) [fs, process, error] -> Result[Str] {
  without fs, process {
    run true ?
    return file.read_text()
  }
}

print ${load(p"x")?}
""",
  )?
  assert count(stderr, "err[check.effect-violation]") == 2, stderr
  assert "`run` requires the `process` effect, which `without process` excludes here" in stderr, stderr
  assert "requires the `fs` effect, which `without fs` excludes here" in stderr, stderr
  assert "this region excludes `fs`" in stderr, stderr
}

test test_without_rejects_callees_by_their_contract { |ctx|
  let stderr = violations(
    ctx,
    r"""proc fetch(url: Str) [net, error] -> Result[Int] {
  net.request({method: "GET", url: url})?.status
}

proc inner(url: Str) -> Result[Int] {
  fetch(url)
}

proc outer(url: Str) -> Result[Int] {
  inner(url)
}

proc main() [net, error] {
  without net {
    let direct = fetch("https://example.test/")?
    let inferred = outer("https://example.test/")?
    print $direct $inferred
  }

  print ${fetch("https://example.test/")?}
}
""",
  )?
  assert count(stderr, "err[check.effect-violation]") == 2, stderr
  assert "effect `net` required by `fetch` is excluded by `without net`" in stderr, stderr
  assert "effect `net` required by `outer` is excluded by `without net`" in stderr, stderr
}

test test_without_counts_io_as_every_effect_it_implies { |ctx|
  let stderr = violations(
    ctx,
    r"""proc anything() [io] {
  print "any host effect"
}

proc pure_work() [] {
  print "no host effect"
}

proc main() [io, error] {
  without net {
    pure_work()?
    anything()?
  }

  without io {
    anything()?
  }
}
""",
  )?
  assert count(stderr, "err[check.effect-violation]") == 2, stderr
  assert "effect `io` required by `anything` implies `net`, which `without net` excludes" in stderr, stderr
  assert "effect `io` required by `anything` is excluded by `without io`" in stderr, stderr
}

test test_without_needs_a_checked_contract_even_in_unrestricted_code { |ctx|
  let root = test.temp_dir(ctx, name: "without-unrestricted")?
  let helper = fp"{root}/helper.xsh"
  helper.write("""##! Helper whose export has no effect clause.

## Does anything.
export proc anything() {
  print "anything"
}
""")

  # An export without a clause has its effects inferred, so the region can
  # hold it to its bound.
  let inferred = test.run_script(
    ctx,
    r"""use helper

without net {
  helper.anything()
}
""",
    [],
    {XSH_MODULE_PATH: root},
  )?
  assert inferred.status == 0, f"{inferred.stdout}{inferred.stderr}"

  # A contract entry without a clause accepts any effects, so nothing is
  # known about the callee and the region cannot hold it.
  let source = r"""type Helper = module {
  export proc anything()
}

let helper = module.load(p"HELPER")?.require(Helper)?
helper.anything()
without net {
  helper.anything()
}
""".replace("HELPER", with: helper.display())
  let output = test.run_script(ctx, source)?
  assert output.status == 2, f"{output.stdout}{output.stderr}"
  assert "err[check.effect-violation]" in output.stderr, output.stderr
  assert "`without net` cannot hold" in output.stderr, output.stderr
  assert count(output.stderr, "err[check.effect-violation]") == 1, output.stderr
  assert ":8:3" in output.stderr, output.stderr
}

test test_nested_without_regions_add_up { |ctx|
  let stderr = violations(
    ctx,
    r"""proc main() [fs, net, error] {
  without net {
    without fs {
      let _ = p"a".read_text()?
      let _ = net.request({method: "GET", url: "https://example.test/"})?
    }

    let _ = p"b".read_text()?
    let _ = net.request({method: "GET", url: "https://example.test/"})?
  }

  let _ = net.request({method: "GET", url: "https://example.test/"})?
}
""",
  )?
  assert count(stderr, "err[check.effect-violation]") == 3, stderr
  assert count(
    stderr,
    "err[check.effect-violation]: `net.request` requires the `net` effect, which `without net` excludes here",
  ) == 2, stderr
  assert count(
    stderr,
    "err[check.effect-violation]: method `read_text` requires the `fs` effect, which `without fs` excludes here",
  ) == 1, stderr
  assert ":4:15" in stderr and ":5:15" in stderr and ":9:13" in stderr, stderr
}

test test_without_covers_callbacks_written_in_the_region { |ctx|
  let stderr = violations(
    ctx,
    r"""proc size(file: Path) [fs, error] -> Result[Int] {
  file.read_text()?.byte_len()
}

proc main() [fs, error] {
  without fs {
    let sizes = [p"a", p"b"] |> map { |file| size(file) ?? 0 } |> collect
    print ${sizes.len()}
  }
}
""",
  )?
  assert "effect `fs` required by `size` is excluded by `without fs`" in stderr, stderr
}

# A violation the proc's own clause already reports is reported once, and a
# region does not change what the enclosing proc is inferred to need.
test test_without_sits_beside_the_proc_clause { |ctx|
  let stderr = violations(
    ctx,
    r"""proc fenced() [fs, error] {
  without net {
    let _ = net.request({method: "GET", url: "https://example.test/"})?
  }
}

proc fetches() -> Result[Int] {
  without net {
    print "offline part"
  }

  net.request({method: "GET", url: "https://example.test/"})?.status
}

proc offline() [error] {
  print ${fetches()?}
}
""",
  )?
  assert count(stderr, "err[check.effect-violation]") == 2, stderr
  assert "`net.request` requires the `net` effect" in stderr, stderr
  assert "excludes" not in stderr, stderr
  assert "effect `net` required by `fetches` is not in caller's declared effects" in stderr, stderr
}

test test_without_error_is_rejected { |ctx|
  let stderr = violations(
    ctx,
    r"""without error, net {
  print "bounded"
}
""",
  )?
  assert "err[check.without-effect]" in stderr, stderr
  assert "bound errors with `try { ... }`" in stderr, stderr
}

test test_without_formats_and_checks_through_the_tools { |ctx|
  let script = test.temp_file(ctx, name: "bounded.xsh")?
  script.write("""proc main() [fs, net, error] {
  without   net ,process {
      print "offline"
  }
}
""")
  let formatted = run.capture --text "xsht" fmt $script ?
  assert formatted.status.exited_with(0), formatted.stderr
  assert script.read_text()? == """proc main() [fs, net, error] {
  without net, process {
    print "offline"
  }
}
"""

  let checked = run.capture --text "xsht" check $script ?
  assert checked.status.exited_with(0), checked.stderr
  let ran = run.capture --text "xsh" $script ?
  assert ran.status.exited_with(0), ran.stderr
  assert ran.stdout == "offline\n", ran.stdout
}

type ToolResult = {ok: Bool, out: Str}

const HELPER = """##! Clock helpers.
## Reads the clock.
export proc clock() -> Int {
  let _ = time.now()
  42
}

## Forwards to the clock.
export proc stamp() -> Int { clock() }

## Yields one clock-derived value.
export stream ticks() -> Stream[Int] {
  let _ = time.now()
  yield 1
}

## Promises the clock even though it does not read it.
export proc published() [time] -> Int { 42 }
"""

# Writes `helper.xsh` and `main.xsh` side by side so `use helper` resolves
# file-relative, then runs `tool` on the entry. `@ROOT@` in `main` names their
# directory. An empty `xsht-config.ini` there gives `xsht` its defaults instead
# of the configuration of the directory the suite runs from.
proc run_tool(ctx: TestContext, tool: Str, args: List[Str], main: Str) [fs, process, error] -> Result[ToolResult] {
  let root = test.temp_dir(ctx, name: "exported-effects")?
  fp"{root}/xsht-config.ini".write("")
  fp"{root}/helper.xsh".write(HELPER)
  let entry = fp"{root}/main.xsh"
  entry.write(main.replace("@ROOT@", root.display()))
  let out = run.capture --text $tool @args $entry ?
  {ok: out.status.exited_with(0), out: out.stdout + out.stderr}
}

test test_importer_sees_inferred_export_effects { |ctx|
  let result = run_tool(
    ctx,
    "xsh",
    [],
    """use helper
proc caller() [time] -> Int { helper.stamp() }
print \${caller()}
""",
  )?
  assert result.ok, result.out
  assert result.out == "42\n"
}

test test_importer_restriction_rejects_inferred_export_effects { |ctx|
  let result = run_tool(ctx, "xsh", [], "use helper\nproc caller() [] -> Int { helper.stamp() }\n")?
  assert ! result.ok
  assert "check.effect-violation" in result.out
  assert "time" in result.out
}

test test_exported_stream_effects_are_inferred { |ctx|
  let allowed = run_tool(
    ctx,
    "xsh",
    [],
    """use helper
proc caller() [time] -> Int { helper.ticks().collect().len() }
print \${caller()}
""",
  )?
  assert allowed.ok, allowed.out
  let rejected = run_tool(ctx, "xsh", [], "use helper\nproc caller() [] -> Stream[Int] { helper.ticks() }\n")?
  assert ! rejected.ok
  assert "time" in rejected.out
}

test test_declared_export_clause_stays_an_upper_bound { |ctx|
  let result = run_tool(ctx, "xsh", [], "use helper\nproc caller() [] -> Int { helper.published() }\n")?
  assert ! result.ok
  assert "check.effect-violation" in result.out
  assert "time" in result.out
}

test test_inferred_export_effects_reach_through_module_values { |ctx|
  let main = """use helper
let clocks = helper
proc forwarding() -> Int { clocks.clock() }
proc caller() [] -> Int { forwarding() }
"""
  let rejected = run_tool(ctx, "xsh", [], main)?
  assert ! rejected.ok
  assert "check.effect-violation" in rejected.out
  assert "time" in rejected.out
  let allowed = run_tool(ctx, "xsh", [], main.replace("[]", "[time]") + "print \${caller()}\n")?
  assert allowed.ok, allowed.out
  assert allowed.out == "42\n"
}

test test_inferred_exports_satisfy_module_contracts { |ctx|
  let main = """use helper
type Clock = module {
  export proc clock() [time] -> Int
}
type AnyClock = module {
  export proc clock() -> Int
}
type FileClock = module {
  export proc clock() [fs] -> Int
}
proc exact(source: Clock) [time] -> Int { source.clock() }
proc loose(source: AnyClock) -> Int { source.clock() }
let loaded = module.load(p"@ROOT@/helper.xsh")?
print \${exact(helper)} \${loose(helper)} \${exact(loaded.require(Clock)?)}
print \${loaded.require(FileClock) is Err(_)}
"""
  let result = run_tool(ctx, "xsh", [], main)?
  assert result.ok, result.out
  assert result.out == "42 42 42\ntrue\n"
}

test test_lint_reports_redundant_private_clause_across_modules { |ctx|
  let main = """##! Entry with clauses that name exactly their inferred effects.
use helper
proc caller() [time] -> Int { helper.stamp() }
## Keeps its clause as an API contract.
export proc published_caller() [time] -> Int { helper.stamp() }
proc main() [time] { print \${caller() + published_caller()} }
"""
  let result = run_tool(ctx, "xsht", ["lint"], main)?
  let flagged = result.out.lines() |> where { |line| "lint.prefer-inferred-private-effects" in line } |> count()
  assert flagged == 1, result.out
  assert "main.xsh:3:" in result.out, result.out
}

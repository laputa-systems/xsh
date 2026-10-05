# An explicit erased descriptor keeps runtime validation observable rather than
# admitting its fields as preparation facts.
pure dynamic_cli_schema(schema: Record) -> Record {
  schema
}

# Coverage for the `cli` argument policy.
#
# Every public entry is exercised here: `cli.parse`, `cli.parse_full`,
# `cli.applet`, `cli.usage`, `cli.commands`, `cli.commands_rootless` (the
# four-argument spelling is the only one that takes a rootless default), and
# `cli.tokens`. Their native implementation in `src/modules/cli.rs` is authoritative.
#
# The value machinery — types, defaults, repeated values, choices, numeric
# bounds, conflicts, requires, required groups, path checks — is shared by every
# entry, and most of it is written below against a command's own option schema,
# which a command descriptor reads exactly as `parse` reads one. The two things
# a command cannot reach are the environment fallback (the command walk supplies
# an empty environment) and the policy selection: `cli.parse` and `cli.applet`
# are the same walk with three deltas, and each delta is pinned against the
# strict spelling that differs from it.
#
# The usage text is labeled with the command name, which `parse`, `parse_full`,
# and `applet` default from the evaluator's command name at call time. The
# cases that assert a label either pass the label explicitly or run a nested
# script whose name is known.
#
# Command records are read field by field with `get` rather than through a
# declared record type, because the fields present depend on the descriptor
# under test.
type TokenRecord = {kind: Str, name: Str, value: Str}

# The message of a rejection, or the empty string when the call succeeded.
# `test.error_kind` compares kinds only, so message parity is asserted through
# this.
pure failure_message(result: Result[Record]) -> Str {
  match result {
    Ok(_) => ""
    Err(failure) => failure.message
  }
}

# The token spellings of a `cli.tokens` result, one per token, as
# `kind:name:value`, or `rejected` when the call was refused.
pure token_spellings(result: Result[List[TokenRecord]]) -> Str {
  if let Ok(items) = result {
    let rendered = [f"{token.kind}:{token.name}:{token.value}" for token in items]
    rendered.join(",")
  } else {
    "rejected"
  }
}

# A command result's rest list, joined, or the empty string when the call was
# refused or the command declared no rest field.
pure rest_field(result: Result[Record], name: Str) -> Result[Str] {
  if let Ok(parsed) = result {
    (parsed.get(name) ?? []).require(List[Str])?.join(",")
  } else {
    ""
  }
}

test test_cli_usage_renders_the_command_line_and_its_sections {
  # Positionals contribute to the command line in sorted schema-name order, and
  # each is bare when it is required and bracketed when it is not. A positional
  # with no explicit `required` and no default is required; an explicit
  # `required: false` always wins, as it does everywhere else.
  assert cli.usage(
    {zebra: {positional: true}, alpha: {positional: true, required: false}},
    "demo",
  ) == """usage: demo [ALPHA] ZEBRA [OPTIONS]

options:
  -h, --help  show this help"""

  # A form supplies the label a positional shows, and a `...NAME` form makes it
  # repeated and therefore optional. Labels are sorted with their names, so
  # `rest` comes before `root`.
  assert cli.usage(
    {root: {positional: true, form: "ROOT"}, rest: {positional: true, form: "...RAW"}},
    "demo",
  ) == """usage: demo [...RAW] ROOT [OPTIONS]

options:
  -h, --help  show this help"""

  # A form that names a bare positional marks it positional, and a default makes
  # it optional.
  assert cli.usage({kind: {form: "KIND", default: "rust"}}, "dev") == """usage: dev [KIND] [OPTIONS]

options:
  -h, --help  show this help"""

  # The `arguments:` section lists the visible positionals that carry help, and
  # it comes before the options. A hidden option appears in neither section.
  assert cli.usage(
    {secret: {kind: "Bool", hidden: true}, name: {positional: true, help: "a name"}},
    "demo",
  ) == """usage: demo NAME [OPTIONS]

arguments:
  NAME  a name

options:
  -h, --help  show this help"""

  # An option line lists every short name, then every long name, then the value
  # placeholder — the descriptor's own metavar when its form names one and the
  # option's name in upper case otherwise — and a value that may be omitted is
  # bracketed after `=`.
  assert cli.usage(
    {
      out: {
        kind: "Path",
        short: [
          "o",
        ],
        help: "where to write",
      },
      count: {
        kind: "Int",
      },
      mode: {
        kind: "Str",
        optional_value: true,
        form: "--mode[=MODE]",
      },
    },
    "demo",
  ) == """usage: demo [OPTIONS]

options:
  --count COUNT
  --mode[=MODE]
  -o, --out OUT  where to write
  -h, --help  show this help"""

  # A deprecated option carries the bare word when the descriptor only asked
  # for deprecation, and the descriptor's own message when it wrote one.
  assert cli.usage(
    {old: {kind: "Bool", deprecated: true}, older: {kind: "Bool", deprecated: "use --new"}},
    "demo",
  ) == """usage: demo [OPTIONS]

options:
  --old  deprecated
  --older  deprecated: use --new
  -h, --help  show this help"""

  # An empty schema still renders the command line and the implicit help option.
  assert cli.usage({}, "demo") == """usage: demo [OPTIONS]

options:
  -h, --help  show this help"""
}

test test_cli_usage_rejects_a_schema_it_cannot_interpret { |ctx|
  # A renderer reads the schema with the same interpreter the parser uses, so a
  # descriptor that interpreter rejects rejects the whole call with its kind and
  # message, exactly as the baseline's native route does. The rejection is a
  # runtime error rather than a returned value, so it is observed here through a
  # script that propagates it at top level: status 3, the rejection on stderr.
  test.expect(
    ctx,
    """use cli
print cli.usage({count: {kind: "Nope"}}, "demo")
""",
    status: 3,
    stderr: ["cli-parse: unsupported option type `Nope`"],
  )?

  # The schema is read in sorted name order, so the first rejected descriptor
  # reported is the first one the baseline would report — `broken` sorts before
  # `good`.
  let first_rejection = test.run_script(
    ctx,
    """use cli
print cli.usage({good: {kind: "Int"}, broken: 7}, "demo")
""",
  )?

  assert "cli-parse: option `broken` descriptor must be Str or Record, found Int" in first_rejection.stderr
  assert "usage: demo" not in first_rejection.stderr

  # The two help spellings are reserved by the strict reader the parser uses.
  let reserved_help = test.run_script(
    ctx,
    """use cli
print cli.usage({helper: {kind: "Bool", long: ["help"]}}, "demo")
""",
  )?

  assert "cli-parse: `--help` is reserved by cli.parse" in reserved_help.stderr
}

test test_cli_tokens_splits_values_clusters_and_operands {
  # A long option carries an attached value; a short name that takes a value
  # consumes the rest of its cluster or the next argument; a negative number,
  # `-`, and everything after `--` are operands. A token with no value records
  # the empty string rather than a null.
  assert token_spellings(
    cli.tokens(
      [
        "--output=result.txt",
        "-I",
        "include",
        "-1",
        "-",
        "--",
        "-x",
        "-abc",
      ],
      ["I", "output"],
    ),
  ) == "long:output:result.txt,short:I:include,operand:-1:,operand:-:,operand:-x:,operand:-abc:"

  # A cluster splits into one token per name, and the first name that takes a
  # value takes the rest of the cluster as that value.
  assert token_spellings(cli.tokens(["-abc"], [])) == "short:a:,short:b:,short:c:"
  assert token_spellings(cli.tokens(["-ab"], ["a"])) == "short:a:b"

  # A declared value name with nothing left to consume is a rejection rather
  # than a token with an empty value.
  assert token_spellings(cli.tokens(["-I"], ["I"])) == "rejected"
  test.error_kind(cli.tokens(["-I"], ["I"]), "cli-parse")
  assert token_spellings(cli.tokens(["--output"], ["output"])) == "rejected"
  test.error_kind(cli.tokens(["--output"], ["output"]), "cli-parse")

  # A name takes a value only when `value_flags` names it, so an undeclared
  # short name is an ordinary token.
  assert token_spellings(cli.tokens(["-I"])) == "short:I:"

  # An empty long name is not an option and not a value flag, so the whole
  # argument stays an operand, and `--` itself is not a token.
  assert token_spellings(cli.tokens(["--", "-a"], ["a"])) == "operand:-a:"
  assert token_spellings(cli.tokens(["--="], ["output"])) == "operand:--=:"
  assert cli.tokens([])?.len() == 0
}

test test_cli_commands_dispatch_names_aliases_and_forms {
  let schema = {
    build: {
      positionals: [
        "root",
      ],
      types: {
        root: "Path",
      },
      rest: "raw",
      aliases: [
        "b",
      ],
    },
    clean: {
      rest: "raw",
    },
  }

  # A command is dispatched by its name, and the result names both the command
  # that declared the schema and the spelling the argument list used. The
  # fields come back in sorted name order.
  let named = cli.commands(["build", "target/demo", "extra"], schema)?
  assert named.get("command")? == "build"
  assert named.get("action")? == "build"
  assert f"{named.get("root")?.require(Path)?}" == "target/demo"
  assert rest_field(cli.commands(["build", "target/demo", "extra"], schema), "raw")? == "extra"
  assert named.keys().join(",") == "action,command,raw,root"

  # An alias dispatches to the same command, and `action` reports the spelling
  # the list used while `command` reports the declared name.
  let aliased = cli.commands(["b", "target/demo"], schema)?
  assert aliased.get("command")? == "build"
  assert aliased.get("action")? == "b"

  # A lookup key is the command spelling with dashes turned into underscores,
  # while a command is stored under its name exactly as written. A dashed
  # canonical name is therefore reachable neither by its own spelling nor by the
  # underscored one — the baseline's own asymmetry, preserved here.
  assert failure_message(cli.commands(["my-command"], {"my-command": {rest: "raw"}})) == "unknown command `my-command`"
  assert failure_message(cli.commands(["my_command"], {"my-command": {rest: "raw"}})) == "unknown command `my_command`"
  assert cli.commands(["my_command", "one"], {my_command: {rest: "raw"}})?.get("command")? == "my_command"
  assert cli.commands(["my-command"], {my_command: {rest: "raw"}})?.get("command")? == "my_command"

  # An alias is keyed by the same normalization, so a dashed alias spelling is
  # how a dashed canonical name is reached.
  let via_alias = cli.commands(["mc", "one"], {"my-command": {rest: "raw", aliases: ["mc"]}})?
  assert via_alias.get("command")? == "my-command"
  assert via_alias.get("action")? == "mc"

  # A form descriptor spells the positionals and the rest compactly, lowercased,
  # and the rest name it declares wins over the descriptor's own `rest` field. A
  # form token is not a turn of the argument list: the command word and any
  # token that looks like an option are skipped.
  assert rest_field(
    cli.commands(
      ["push", "R", "a", "b"],
      {push: {form: "push ROOT ...RAW", rest: "ignored", types: {root: "Str"}}},
    ),
    "raw",
  )? == "a,b"

  # A command that matches nothing is reported with the token that named
  # nothing, and an empty list has nothing to report.
  assert failure_message(cli.commands(["nope"], schema)) == "unknown command `nope`"
  assert failure_message(cli.commands([], schema)) == "missing command"
  test.error_kind(cli.commands([], schema), "cli-commands")

  # An alias another command already claims is rejected while the schema is
  # interpreted, before any argument is read.
  assert failure_message(
    cli.commands(
      ["b"],
      dynamic_cli_schema({build: {rest: "raw", aliases: ["b"]}, bale: {rest: "raw", aliases: ["b"]}}),
    ),
  ) == "duplicate command alias `b`"
}

test test_cli_commands_rootless_and_fallback_routing {
  let schema = {build: {rest: "raw"}, clean: {positionals: ["root"], types: {root: "Str"}}}

  # A rootless default takes a first token that names no command, and the whole
  # argument list — the token that named nothing included — becomes its
  # arguments.
  let rootless = cli.commands(["target/demo", "extra"], "build", {build: {rest: "raw"}})?
  assert rootless.get("command")? == "build"
  assert rootless.get("action")? == "build"
  assert (rootless.get("raw") ?? []).join(",") == "target/demo,extra"

  # A fallback command is consulted before the rootless default, and it too
  # takes the whole argument list, with the token that named nothing as both its
  # own name and its first argument.
  let taken = cli.commands(["plain", "one"], "", schema, {rest: "raw"})?
  assert taken.get("action")? == "plain"
  assert taken.get("command")? == "plain"
  assert (taken.get("raw")?.require(List[Str])?).join(",") == "plain,one"

  # A fallback takes a token that looks like a path only when it asks to be
  # `command_like`; a token that looks like a path is one that starts with `/`
  # or `.` or carries `/`.
  assert cli.commands(["plain", "one"], "", schema, {rest: "raw", command_like: true})?.get("action")? == "plain"
  assert failure_message(cli.commands(["./tool"], "", schema, {rest: "raw", command_like: true})) == "unknown command `./tool`"
  assert failure_message(
    cli.commands(
      ["./tool", "a"],
      "",
      schema,
      {positionals: ["tool"], command_like: true},
    ),
  ) == "unknown command `./tool`"

  # With no fallback and no rootless default, the first token is reported
  # itself, and an empty list has nothing to report.
  assert failure_message(cli.commands(["nope"], "", schema)) == "unknown command `nope`"
  assert failure_message(cli.commands([], "", schema)) == "missing command"

  # A rootless default that names no command in the schema is its own rejection.
  assert failure_message(cli.commands(["x"], "nope", {clean: {rest: "raw"}})) == "unknown rootless default command `nope`"
}

test test_cli_commands_convert_positionals_and_collect_the_rest {
  let basic = {t: {positionals: ["n", "where"], types: {n: "Int", where: "Path"}}}

  # The declared type decides the conversion: an integer, a path, a flag
  # spelling, and a duration.
  let parsed = cli.commands(["t", "+12", "src/main.xsh"], basic)?
  assert (parsed.get("n") ?? null) == 12
  assert f"{parsed.get("where")?.require(Path)?}" == "src/main.xsh"
  assert cli.commands(["t", "yes"], {t: {positionals: ["n"], types: {n: "Bool"}}})?.get("n")? == true
  assert cli.commands(["t", "0"], {t: {positionals: ["n"], types: {n: "Bool"}}})?.get("n")? == false
  assert cli.commands(["t", "250ms"], {t: {positionals: ["n"], types: {n: "Duration"}}})?.get("n")? == 250ms

  # A value the declared type cannot hold is rejected with the operand's own
  # index in the operand list, and its own type spelling.
  assert failure_message(cli.commands(["t", "1x", "p"], basic)) == "positional `n` expects Int at argv[0], got `1x`"
  assert failure_message(cli.commands(["t", "-1"], {t: {positionals: ["n"], types: {n: "UInt"}}})) == "positional `n` expects UInt at argv[0], got `-1`"
  assert failure_message(cli.commands(["t", "maybe"], {t: {positionals: ["n"], types: {n: "Bool"}}})) == "positional `n` expects Bool at argv[0], got `maybe`"
  assert failure_message(cli.commands(["t", "soon"], {t: {positionals: ["n"], types: {n: "Duration"}}})) == "positional `n` expects Duration at argv[0], got `soon`"

  # A type spelling the command reader cannot convert is a rejection of the
  # schema, reported once, before any argument is converted.
  assert failure_message(cli.commands(["t", "x"], dynamic_cli_schema({t: {positionals: ["n"], types: {n: "Weird"}}}))) == "unsupported command positional type `Weird`"
  assert failure_message(
    cli.commands(["t", "x"], dynamic_cli_schema({t: {positionals: ["n"], types: {n: "List[Str]"}}})),
  ) == "command `t` type for `n` cannot be List"
  assert failure_message(cli.commands(["t", "x"], dynamic_cli_schema({t: {positionals: ["n"], types: {n: 7}}}))) == "command `t` type for `n` must be Str"

  # An operand with no positional left is rejected, and the rest name takes what
  # remains once the declared positionals are filled. Positionals consume
  # operands in declared order, so the first one missing a value is the one
  # reported.
  assert failure_message(cli.commands(["t", "1", "x", "y"], basic)) == "unexpected positional argument for command `t`: y"
  assert failure_message(cli.commands(["t"], basic)) == "missing positional `n` for command `t`"
  assert rest_field(cli.commands(["t", "one", "two"], {t: {rest: "raw"}}), "raw")? == "one,two"
  assert failure_message(cli.commands(["t", "one"], {t: {rest: "raw", min_rest: 2}})) == "command `t` expects at least 2 rest arguments"
  assert rest_field(cli.commands(["t", "one", "two"], {t: {rest: "raw", min_rest: 2}}), "raw")? == "one,two"

  # A malformed descriptor is reported against the field that is malformed.
  assert failure_message(cli.commands(["t"], dynamic_cli_schema({t: "Str"}))) == "command `t` descriptor must be Record"
  assert failure_message(cli.commands(["t"], dynamic_cli_schema({t: {positionals: "root"}}))) == "command `t` descriptor field `positionals` must be List[Str]"
  assert failure_message(cli.commands(["t"], dynamic_cli_schema({t: {min_rest: -1}}))) == "command `t` descriptor field `min_rest` cannot be negative"
  assert failure_message(cli.commands(["t"], dynamic_cli_schema({t: {command_like: "yes"}}))) == "command `t` descriptor field `command_like` must be Bool, found Str"

  # Every rejection in this walk is a `cli-commands` error, and none of them
  # carries usage text: attaching it is entry-point policy, and this walk
  # renders none.
  test.error_kind(cli.commands(["t", "1", "x", "y"], basic), "cli-commands")
  assert "usage:" not in failure_message(cli.commands(["t", "1", "x", "y"], basic))
  assert "usage:" not in failure_message(cli.commands([], basic))
}

test test_cli_command_options_split_values_and_defaults {
  let schema = {
    go: {
      positionals: [
        "root",
      ],
      options: {
        verbose: {
          kind: "Bool",
          short: [
            "v",
          ],
        },
        tag: {
          kind: "Str",
          repeated: true,
        },
        mode: {
          kind: "Str",
          default: "slow",
        },
        level: {
          kind: "Str",
          optional_value: true,
          default: "info",
        },
      },
    },
  }

  # A command's options are read exactly as `parse` reads them, and the result
  # carries one field per option name beside the command fields.
  let parsed = cli.commands(["go", "r", "-v", "--tag=a", "--tag", "b", "--mode", "fast"], schema)?
  assert (parsed.get("verbose") ?? null) == true
  assert (parsed.get("tag")?.require(List[Str])?).join(",") == "a,b"
  assert (parsed.get("mode") ?? null) == "fast"
  assert (parsed.get("level") ?? null) == "info"
  assert parsed.keys().join(",") == "action,command,level,mode,root,tag,verbose"

  # A value may be carried separately or inline, an option that may omit its
  # value falls back to its default when nothing supplies one, and an option
  # with a default starts there.
  assert (cli.commands(["go", "r", "--tag", "b"], schema)?.get("tag")?.require(List[Str])?).join(",") == "b"
  assert (cli.commands(["go", "r", "--level"], schema)?.get("level") ?? null) == "info"
  assert (cli.commands(["go", "r", "--level=deep"], schema)?.get("level") ?? null) == "deep"
  let defaults = cli.commands(["go", "r"], schema)?
  assert (defaults.get("mode") ?? null) == "slow"
  assert (defaults.get("verbose") ?? null) == false

  # A repeated option that declares a default starts at that list and appends.
  let toppers = cli.commands(
    ["go", "r", "--topper=x"],
    {go: {positionals: ["root"], options: {topper: {kind: "Str", repeated: true, default: ["base"]}}}},
  )?.get("topper") ?? []
  assert toppers.join(",") == "base,x"

  # A flag given a value inline records the value the value reader produces for
  # that text rather than the flag's own `true`.
  assert cli.commands(["go", "r", "--v=1"], {go: {positionals: ["root"], options: {v: {kind: "Bool"}}}})?.get("v")? == true

  # A declared option consumes the next argument as its value even when that
  # argument looks like an option, and the value reader then refuses a value
  # that starts with `--` — so the option is the one reported, not the one it
  # swallowed.
  assert failure_message(cli.commands(["go", "r", "--mode", "--tag"], schema)) == "missing value for --mode at argv[0]"
  assert failure_message(cli.commands(["go", "r", "--tag"], schema)) == "missing value for --tag at argv[0]"

  # A second value for an option that does not repeat is a rejection, and its
  # index is the one it has in the split option list rather than in `argv`.
  assert failure_message(cli.commands(["go", "r", "-v", "-v"], schema)) == "duplicate argument at argv[1]: -v"
  test.error_kind(cli.commands(["go", "r", "-v", "-v"], schema), "cli-parse")

  # A token that names no declared option is an operand, not an unknown option:
  # the split hands it to the positionals, and the positional reader rejects it
  # when nothing is left to fill. A short cluster is claimed only by a name the
  # command actually declares, and only a declared name carries a value, so
  # `-tag` is an operand rather than `-t ag`.
  assert failure_message(cli.commands(["go", "r", "--nope"], schema)) == "unexpected positional argument for command `go`: --nope"
  assert failure_message(cli.commands(["go", "r", "--tag", "a", "extra"], schema)) == "unexpected positional argument for command `go`: extra"
  assert failure_message(cli.commands(["go", "r", "-tag", "b"], schema)) == "unexpected positional argument for command `go`: -tag"

  # `--` ends option splitting only when the command declares options. A
  # command that declares none takes every argument as an operand, `--`
  # included — the baseline's own early return.
  assert failure_message(cli.commands(["go", "r", "--", "--tag", "x"], schema)) == "unexpected positional argument for command `go`: --tag"
  assert rest_field(cli.commands(["build", "--", "-x"], {build: {rest: "raw"}}), "raw")? == "--,-x"
}

test test_cli_command_options_validate_values_and_relationships {
  # Conflicting, required, and grouped options are checked after the walk, in
  # sorted schema-name order, so the first relationship that fails is the one
  # reported.
  assert failure_message(
    cli.commands(
      ["go", "r", "--a", "--b"],
      {go: {positionals: ["root"], options: {a: {kind: "Bool", conflicts: ["b"]}, b: {kind: "Bool"}}}},
    ),
  ) == "--a conflicts with --b"
  assert failure_message(
    cli.commands(
      ["go", "r", "--a"],
      {go: {positionals: ["root"], options: {a: {kind: "Bool", requires: ["b"]}, b: {kind: "Bool"}}}},
    ),
  ) == "--a requires --b"
  assert failure_message(
    cli.commands(
      ["go", "r"],
      {go: {positionals: ["root"], options: {a: {kind: "Bool", required: true}}}},
    ),
  ) == "missing required argument --a"
  assert failure_message(
    cli.commands(
      ["go", "r"],
      {
        go: {
          positionals: [
            "root",
          ],
          options: {
            a: {
              kind: "Bool",
              required_group: "pick",
            },
            c: {
              kind: "Bool",
              required_group: "pick",
            },
          },
        },
      },
    ),
  ) == "one of required group `pick` is required: --a, --c"
  assert failure_message(
    cli.commands(
      ["go", "r", "--a"],
      {
        go: {
          positionals: [
            "root",
          ],
          options: {
            a: {
              kind: "Bool",
              required_group: "pick",
            },
            c: {
              kind: "Bool",
              required_group: "pick",
            },
          },
        },
      },
    ),
  ) == ""

  # Numeric bounds, choices, and value types are the option reader's own.
  assert failure_message(
    cli.commands(
      ["go", "r", "--mode", "quick"],
      {go: {positionals: ["root"], options: {mode: {kind: "Str", choices: ["slow", "fast"]}}}},
    ),
  ) == "option --mode expects one of slow|fast, got `quick` at argv[0]"
  assert failure_message(
    cli.commands(
      ["go", "r", "--n", "2"],
      {go: {positionals: ["root"], options: {n: {kind: "Int", min: 3}}}},
    ),
  ) == "option --n expects value >= 3"
  assert failure_message(
    cli.commands(
      ["go", "r", "--n", "9"],
      {go: {positionals: ["root"], options: {n: {kind: "Int", max: 3}}}},
    ),
  ) == "option --n expects value <= 3"
  assert failure_message(
    cli.commands(
      ["go", "r", "--n", "0"],
      {go: {positionals: ["root"], options: {n: {kind: "Int", positive: true}}}},
    ),
  ) == "option --n expects a positive integer"
  assert failure_message(
    cli.commands(
      ["go", "r", "--n", "0"],
      {go: {positionals: ["root"], options: {n: {kind: "Int", nonzero: true}}}},
    ),
  ) == "option --n expects a non-zero integer"
  assert failure_message(
    cli.commands(
      ["go", "r", "--n", "x"],
      {go: {positionals: ["root"], options: {n: {kind: "Int"}}}},
    ),
  ) == "option --n expects Int at argv[1], got `x`"

  # The command walk supplies an empty environment, so an option whose
  # descriptor names one still starts at its default, and a deprecated option
  # is accepted with no warning kept for it.
  assert cli.commands(
    ["go", "r"],
    {go: {positionals: ["root"], options: {mode: {kind: "Str", env: "MODE", default: "slow"}}}},
  )?.get("mode")? == "slow"
  assert cli.commands(
    ["go", "r", "--old"],
    {go: {positionals: ["root"], options: {old: {kind: "Bool", deprecated: true}}}},
  )?.get("old")? == true

  # A command's option schema is read under the strict policy, so the two help
  # spellings are reserved here as well — and a rejection of the *schema* is a
  # `cli-parse` error, not a `cli-commands` one.
  assert failure_message(
    cli.commands(["go", "r"], dynamic_cli_schema({go: {options: {helpful: {kind: "Bool", short: ["h"]}}}})),
  ) == "`-h` is reserved by cli.parse"
  test.error_kind(
    cli.commands(["go", "r"], dynamic_cli_schema({go: {options: {helpful: {kind: "Bool", short: ["h"]}}}})),
    "cli-parse",
  )
  assert failure_message(
    cli.commands(["go", "r"], dynamic_cli_schema({go: {options: {helper: {kind: "Bool", long: ["help"]}}}})),
  ) == "`--help` is reserved by cli.parse"
}

test test_cli_command_option_path_constraints { |ctx|
  let present = test.temp_file(ctx, name: "cli-present.txt", contents: b"text")?
  let root = test.temp_dir(ctx, name: "cli-root")?
  let missing = test.temp_path(ctx, name: "cli-missing")

  # A descriptor that asks for none of the three constraints probes nothing, so
  # a value that does not exist is accepted.
  assert failure_message(cli.commands(["go", missing.display()], {go: {positionals: ["target"]}})) == ""

  # The three constraints are asked in this order — an existing path, then a
  # file, then a directory — so a value that fails several of them is reported
  # against the first.
  assert failure_message(
    cli.commands(
      ["go", "--target", missing.display()],
      {go: {options: {target: {kind: "Path", exists: true}}}},
    ),
  ) == f"option --target expects an existing path: {missing}"
  assert failure_message(
    cli.commands(
      ["go", "--target", root.display()],
      {go: {options: {target: {kind: "Path", file: true}}}},
    ),
  ) == f"option --target expects a file path: {root}"
  assert failure_message(
    cli.commands(
      ["go", "--target", present.display()],
      {go: {options: {target: {kind: "Path", dir: true}}}},
    ),
  ) == f"option --target expects a directory path: {present}"
  assert failure_message(
    cli.commands(
      ["go", "--target", missing.display()],
      {go: {options: {target: {kind: "Path", exists: true, file: true}}}},
    ),
  ) == f"option --target expects an existing path: {missing}"

  # A present file and a present directory satisfy every spelling that asks for
  # them, and a symbolic link is resolved the way the baseline's probes resolve
  # it: a link to a present target answers for the target, while a link to a
  # missing one is not an existing path.
  assert failure_message(
    cli.commands(
      ["go", "--target", present.display()],
      {go: {options: {target: {kind: "Path", exists: true, file: true}}}},
    ),
  ) == ""
  assert failure_message(
    cli.commands(
      ["go", "--target", root.display()],
      {go: {options: {target: {kind: "Path", exists: true, dir: true}}}},
    ),
  ) == ""
  let file_link = test.temp_path(ctx, name: "cli-file-link")
  file_link.symlink(to: present)
  assert failure_message(
    cli.commands(
      ["go", "--target", file_link.display()],
      {go: {options: {target: {kind: "Path", exists: true, file: true}}}},
    ),
  ) == ""
  let dangling = test.temp_path(ctx, name: "cli-dangling")
  dangling.symlink(to: missing)
  assert failure_message(
    cli.commands(
      ["go", "--target", dangling.display()],
      {go: {options: {target: {kind: "Path", exists: true}}}},
    ),
  ) == f"option --target expects an existing path: {dangling}"
}

test test_cli_parse_returns_values_and_asks_for_help {
  # A supplied value keeps the spelling it was given, takes the declared type,
  # or is `true` for a flag that appeared. An entry no argument supplies falls
  # back to its default, to `false` for a flag, and to `null` for an optional
  # value that declares neither — a value the walk never confuses with an
  # absent one.
  let parsed = cli.parse(
    ["--name", "x", "--count=2", "--verbose"],
    {name: "Str", count: "Int", verbose: "Bool", mode: {kind: "Str", optional_value: true}},
    "demo",
  )?
  assert (parsed.get("name") ?? "") == "x"
  assert (parsed.get("count") ?? 0) == 2
  assert parsed.get("verbose")? == true
  assert (parsed.get("mode") ?? "sentinel") == null

  let sparse = cli.parse([], {verbose: "Bool", name: {kind: "Str", default: "d"}}, "demo")?
  assert sparse.get("verbose")? == false
  assert (sparse.get("name") ?? "") == "d"

  # A positional consumes the operand in its place, a repeated positional
  # collects the rest, and `--` ends option parsing so that a later `--name` is
  # an operand rather than an option.
  let operands = cli.parse(
    ["one", "two", "three"],
    {first: {positional: true}, rest: {positional: true, form: "...RAW"}},
    "demo",
  )?
  assert (operands.get("first") ?? "") == "one"
  assert (operands.get("rest") ?? []).join(",") == "two,three"
  assert (cli.parse(["--", "--name"], {name: {positional: true}}, "demo")?.get("name") ?? "") == "--name"

  # A rejection carries its kind and the usage text appended to its message,
  # and a duplicate is the same shape with the second spelling's argv index.
  test.error_kind(cli.parse(["--nope"], {name: "Str"}, "demo"), "cli-parse")
  assert failure_message(cli.parse(["--nope"], {name: "Str"}, "demo")) == """unknown argument at argv[0]: --nope

usage: demo [OPTIONS]

options:
  --name NAME
  -h, --help  show this help"""
  assert failure_message(cli.parse(["--name", "a", "--name", "b"], {name: "Str"}, "demo")) == """duplicate argument at argv[2]: --name

usage: demo [OPTIONS]

options:
  --name NAME
  -h, --help  show this help"""

  # A schema the reader cannot interpret rejects before the walk, so its
  # rejection is a `cli-parse` one without the usage text; the same reading
  # reserves the `-h` short for `parse` alone.
  assert failure_message(cli.parse([], dynamic_cli_schema({count: {kind: "Nope"}}), "demo")) == "unsupported option type `Nope`"
  assert failure_message(cli.parse(["-h"], dynamic_cli_schema({handle: {short: "h", kind: "Bool"}}), "demo")) == "`-h` is reserved by cli.parse"

  # `--help`, `-h`, and any short cluster carrying an unclaimed `h` ask for
  # help: the rejection's kind is `cli-help` and its message is the usage text
  # alone.
  test.error_kind(cli.parse(["--help"], {}, "demo"), "cli-help")
  assert failure_message(cli.parse(["--help"], {}, "demo")) == """usage: demo [OPTIONS]

options:
  -h, --help  show this help"""
  test.error_kind(cli.parse(["-h"], {}, "demo"), "cli-help")
  test.error_kind(cli.parse(["-vh"], {v: {short: "v", kind: "Bool"}}, "demo"), "cli-help")

  # `--` ends option parsing before any help spelling is read, so the spelling
  # after it is an operand the schema has no place for.
  assert failure_message(cli.parse(["--", "-h"], {}, "demo")) == """unexpected positional argument at argv[1]: -h

usage: demo [OPTIONS]

options:
  -h, --help  show this help"""
}

test test_cli_parse_full_reports_sources_and_warnings {
  # `parse_full` reports the record `parse` returns under `values`, where every
  # value came from under `sources`, and the walk's warnings. A value an
  # argument supplied names `argv`, an environment name it read names `env`, a
  # descriptor default names `default` — which is also what a flag that
  # appeared nowhere reports, its `false` being the descriptor's — and an entry
  # nothing supplied names `absent`.
  assert (json.encode(
    cli.parse_full(
      ["--name", "x"],
      {name: {kind: "Str", env: "DEMO_NAME"}},
      {DEMO_NAME: "from-env"},
      "demo",
    )?,
  ) ?? "") == """{"sources":{"name":"argv"},"values":{"name":"x"},"warnings":[]}"""
  assert (json.encode(cli.parse_full([], {verbose: "Bool", mode: {kind: "Str", optional_value: true}}, {}, "demo")?) ?? "") == """{"sources":{"mode":"absent","verbose":"default"},"values":{"mode":null,"verbose":false},"warnings":[]}"""

  # An environment name is consulted only when no argument supplied the option,
  # and it wins over a declared default; a repeated option's default is the
  # list the descriptor declares.
  assert (json.encode(
    cli.parse_full(
      [],
      {name: {kind: "Str", env: "DEMO_NAME", default: "d"}},
      {DEMO_NAME: "from-env"},
      "demo",
    )?,
  ) ?? "") == """{"sources":{"name":"env"},"values":{"name":"from-env"},"warnings":[]}"""
  assert (json.encode(cli.parse_full([], {tag: {kind: "Str", repeated: true, default: ["x", "y"]}}, {}, "demo")?) ?? "") == """{"sources":{"tag":"default"},"values":{"tag":["x","y"]},"warnings":[]}"""

  # An environment value is converted like an argument, so one the declared
  # type rejects rejects the call with the usage text appended.
  assert failure_message(cli.parse_full([], {count: {kind: "Int", env: "DEMO_COUNT"}}, {DEMO_COUNT: "nope"}, "demo")) == """option --count expects Int at argv[0], got `nope`

usage: demo [OPTIONS]

options:
  --count COUNT
  -h, --help  show this help"""

  # A deprecated descriptor warns once when it was used, in the order the walk
  # used it, with the text the descriptor declares when it declares one.
  assert (json.encode(
    cli.parse_full(
      ["--bb", "--aa"],
      {aa: {kind: "Bool", deprecated: true}, bb: {kind: "Bool", deprecated: true}},
      {},
      "demo",
    )?,
  ) ?? "") == """{"sources":{"aa":"argv","bb":"argv"},"values":{"aa":true,"bb":true},"warnings":["option `bb` is deprecated","option `aa` is deprecated"]}"""
  assert (json.encode(cli.parse_full(["--old"], {old: {kind: "Bool", deprecated: "use --new"}}, {}, "demo")?) ?? "") == """{"sources":{"old":"argv"},"values":{"old":true},"warnings":["use --new"]}"""
}

test test_cli_applet_applies_the_three_policy_deltas {
  # One: the short `h` is not reserved, so an applet may claim it. The strict
  # reader rejects that schema outright, and the applet's help line then names
  # `--help` alone because `-h` belongs to the descriptor.
  assert failure_message(cli.parse(["--help"], dynamic_cli_schema({handle: {short: "h", kind: "Bool"}}), "demo")) == "`-h` is reserved by cli.parse"
  assert failure_message(cli.applet(["--help"], {handle: {short: "h", kind: "Bool"}}, "demo")) == """usage: demo [OPTIONS]

options:
  -h, --handle
  --help  show this help"""

  # Two: help is asked for only by a spelling no descriptor claims, so a
  # claimed `h` is the option, alone or inside a cluster, while an unclaimed
  # `h` and `--help` are still help.
  let claimed = cli.applet(["-h"], {handle: {short: "h", kind: "Bool"}}, "demo")?
  assert claimed.get("handle")? == true
  let clustered = cli.applet(
    ["-vh"],
    {v: {short: "v", kind: "Bool"}, handle: {short: "h", kind: "Bool"}},
    "demo",
  )?
  assert clustered.get("v")? == true
  assert clustered.get("handle")? == true
  test.error_kind(cli.applet(["-h"], {}, "demo"), "cli-help")
  test.error_kind(cli.applet(["--help"], {handle: {short: "h", kind: "Bool"}}, "demo"), "cli-help")

  # Three: a scalar spelled twice overwrites instead of rejecting, and a
  # conflict the newly set option declares resets the other option to what its
  # own descriptor would have supplied — a declared default, or `false` for a
  # flag. The reset follows the schema entry that was just set, so a conflict
  # the other option declares is still reported by the relationship check.
  assert (cli.applet(["--name", "a", "--name", "b"], {name: "Str"}, "demo")?.get("name") ?? "") == "b"
  assert failure_message(cli.parse(["--name", "a", "--name", "b"], {name: "Str"}, "demo")) == """duplicate argument at argv[2]: --name

usage: demo [OPTIONS]

options:
  --name NAME
  -h, --help  show this help"""
  let reset_flag = cli.applet(["--bb", "--aa"], {aa: {kind: "Bool", conflicts: ["bb"]}, bb: "Bool"}, "demo")?
  assert reset_flag.get("aa")? == true
  assert reset_flag.get("bb")? == false
  let reset_default = cli.applet(
    ["--bb", "B", "--aa", "A"],
    {aa: {kind: "Str", conflicts: ["bb"], default: "ad"}, bb: {kind: "Str", default: "bd"}},
    "demo",
  )?
  assert (reset_default.get("aa") ?? "") == "A"
  assert (reset_default.get("bb") ?? "") == "bd"
  assert failure_message(cli.parse(["--bb", "--aa"], {aa: {kind: "Bool", conflicts: ["bb"]}, bb: "Bool"}, "demo")) == """--aa conflicts with --bb

usage: demo [OPTIONS]

options:
  --aa
  --bb
  -h, --help  show this help"""

  # The three deltas are the whole difference: a repeated option still
  # collects, and `requires`, required entries, and required groups are
  # enforced by the same check the strict walk runs.
  assert (cli.applet(["--tag", "a", "--tag", "b"], {tag: {kind: "Str", repeated: true}}, "demo")?.get("tag") ?? []).join(
    ",",
  ) == "a,b"
  assert failure_message(cli.applet(["--aa"], {aa: {kind: "Bool", requires: ["bb"]}, bb: "Bool"}, "demo")) == """--aa requires --bb

usage: demo [OPTIONS]

options:
  --aa
  --bb
  -h, --help  show this help"""
  assert failure_message(cli.applet([], {name: {kind: "Str", required: true}}, "demo")) == """missing required argument --name

usage: demo [OPTIONS]

options:
  --name NAME
  -h, --help  show this help"""
  assert failure_message(
    cli.applet([], {aa: {kind: "Bool", required_group: "g"}, bb: {kind: "Bool", required_group: "g"}}, "demo"),
  ) == """one of required group `g` is required: --aa, --bb

usage: demo [OPTIONS]

options:
  --aa
  --bb
  -h, --help  show this help"""
}

test test_cli_parse_names_the_program_and_presents_at_the_boundary { |ctx|
  # The usage label defaults to the name the program was invoked under, read
  # when the entry is called: the nested script is named for the label, so its
  # usage line starts with that name rather than with the renderer's own
  # `command` default. The harness appends a per-run counter to every temp path
  # it hands out, so the label is matched by its prefix and the rest of the
  # usage text is asserted in full; the label an explicit `command` supplies
  # wins over the program's name.
  let help = test.expect(
    ctx,
    """use cli

proc main() [io, error, fs] {
  let _ = cli.parse(["--help"], {})
  print "unreached"
}
""",
    status: 0,
    stdout: [
  "usage: mytool.xsh-",
  """ [OPTIONS]

options:
  -h, --help  show this help
""",
],
    args: [],
    env: {},
    stdin: b"",
    name: "mytool.xsh",
  )?
  assert help.stderr == ""
  assert "unreached" not in help.stdout
  assert failure_message(cli.parse(["--help"], {}, "named")) == """usage: named [OPTIONS]

options:
  -h, --help  show this help"""

  # An unhandled help request is the usage text on stdout with status 0, and an
  # unhandled value rejection is the message with the usage text appended on
  # stderr with status 2. Both are the outer boundary's presentation
  # (`handle_cli_parse_stop`), which stays native; the program stops at the
  # rejection rather than reaching the next statement.
  let rejected = test.expect(
    ctx,
    """use cli

proc main() [io, error, fs] {
  let _ = cli.parse(["--nope"], {name: "Str"})
  print "unreached"
}
""",
    status: 2,
    stderr: [
  "unknown argument at argv[0]: --nope",
  """

usage: mytool.xsh-""",
  """ [OPTIONS]

options:
  --name NAME
  -h, --help  show this help
""",
],
    args: [],
    env: {},
    stdin: b"",
    name: "mytool.xsh",
  )?
  assert rejected.stdout == ""

  # A schema the reader cannot interpret is not a usage rejection, so the
  # boundary leaves it alone: it aborts with status 3 and a traceback whose
  # message carries no usage text. A `nul-path` value rejects the same way,
  # keeping the kind the path cast reports rather than folding it into
  # `cli-parse`; the script route's traceback names that kind and message too,
  # with its own frames and without repeating the appended usage text there —
  # a difference the two presentations above never reach, because both are
  # taken from the error value rather than from the traceback.
  let descriptor = test.expect(
    ctx,
    """use cli

proc main() [io, error, fs] {
  let schema = {count: {kind: "Nope"}}
  let _ = cli.parse([], schema)
}
""",
    status: 3,
    stderr: ["cli-parse: unsupported option type `Nope`"],
    args: [],
    env: {},
    stdin: b"",
    name: "mytool.xsh",
  )?
  assert "usage: mytool" not in descriptor.stderr

  test.expect(
    ctx,
    r"""use cli

proc main() [io, error, fs] {
  let _ = cli.parse(["--out", "a\u{0}b"], {out: "Path"})
}
""",
    status: 3,
    stderr: ["nul-path: paths cannot contain NUL bytes"],
    args: [],
    env: {},
    stdin: b"",
    name: "mytool.xsh",
  )?
}

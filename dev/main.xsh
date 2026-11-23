#!/usr/bin/env -S xsh --
# XSH owns the repository's development lifecycle. Cargo only bootstraps this
# script; each lifecycle operation below uses typed policy and direct argv.
use bench as benchmarks
use build as builds
use cargo_steps
use context as lifecycle
use coverage as coverage_workflow
use dist as distributions
use docs as documentation
use gates
use images
use install as installations
use internal as container_internal
use release as releases
use stage as stages
use test_workflows as tests
use targets as target_policy

type GlobalOptions = {target: Str, rest: List[Str]}

enum TestKind {
    Rust,
    Xsh,
    Linux,
    Macos,
}

type TestOptions = {kind: TestKind, ci: Bool}

type CoverageOptions = {backend: Str}


type DistOptions = {docker: Str, ci: Bool}

enum ReleaseOperation {
    Smoke,
    Package,
    Core,
    Validate,
}

type ReleaseOptions = {action: Str, tag: Str}

pure test_kind(value: Str) -> Result[TestKind] {
  match value {
    "rust" => Rust
    "xsh" => Xsh
    "linux" => Linux
    "macos" => Macos
    else => Err(usage(f"unsupported test target {value}"))
  }
}

pure release_operation(value: Str) -> Result[ReleaseOperation] {
  match value {
    "smoke" => Smoke
    "package" => Package
    "core" => Core
    "validate" => Validate
    else => Err(usage(f"unsupported release action {value}"))
  }
}

enum InternalOperation {
    Dist,
    TestLinux,
    TestLinuxCi,
    TestLinuxSplit,
    TestLinuxCiSplit,
    GateLane,
    GateFull,
    Coverage,
}

pure internal_operation(value: Str) -> Result[InternalOperation] {
  match value {
    "dist" => Dist
    "test-linux" => TestLinux
    "test-linux-ci" => TestLinuxCi
    "test-linux-split" => TestLinuxSplit
    "test-linux-ci-split" => TestLinuxCiSplit
    "gate-lane" => GateLane
    "gate-full" => GateFull
    "coverage" => Coverage
    else => Err(usage(f"unsupported internal operation {value}"))
  }
}

pure help_text() -> Str {
  """XSH development lifecycle

usage: cargo dev COMMAND [OPTIONS]

commands:
  build
  image build  build the Linux OCI layout under .work/ (cargo dev image build)
  profile  build xsh and xsht with line tables and frame pointers
  test-health [--record]  rank the timings of the latest full test run
  check [lint]
  gate [PATH...] [--tier edit|change] [--case ID] [--plan [--json]]
  gate [PATH...] [--case ID] --status
  gate --full
  docs [check]
  lint --fix
  test [xsh|linux|macos] [--ci]
  coverage [--backend native|docker]
  bench --list | --check|--dry-run [--tier TIER] [--workload NAME] [--area AREA] [--skip NAME]
  bench --tier TIER | --workload NAME [--full] | --fast | --syscalls
  dist [--target TRIPLE] [--docker auto|always|never] [--ci]
  install
  release smoke|package|core|validate [--tag RELEASE-TAG]

internal container commands are intentionally omitted from public help.
"""
}

pure usage(message: Str) -> Error {
  error.failure(f"""{message}

{help_text()}""")
}

pure parse_global(args: List[Str]) -> Result[GlobalOptions] {
  var target = ""
  var index = 0

  let rest: List[Str] = collect {
    while index < args.len() {
      let arg = args[index]

      if arg == "--target" {
        return Err(usage("--target requires a target triple")) when index + 1 >= args.len()

        target = args[index + 1]
        index += 2
        continue
      }

      if arg.starts_with("--target=") {
        target = arg.split("=", maxsplit: 1).get(1) ?? ""
        index += 1
        continue
      }

      yield arg
      index += 1
    }
  }

  {target: target, rest: rest}
}

proc dispatch(command: Str, args: List[Str]) [fs, process, env, time, error, io] {
  let ctx = lifecycle.create()?

  match command {
    "image" => {
      guard args == ["build"] else { return Err(usage("image requires build")) }
      return images.build(ctx.root)
    }
    "build" => {
      guard args.is_empty() else {
        return Err(usage("build accepts no arguments"))
      }

      return builds.build(ctx)
    }
    "profile" => {
      guard args.is_empty() else {
        return Err(usage("profile accepts no arguments"))
      }

      let target_dir = fp"{ctx.root}/.work/tmp/profiling-target"
      let environment = target_policy.profiling_environment(ctx.target.triple, target_dir.display())?
      return cargo_steps.run_build(
        stages.command(
          "profile-build",
          ctx.target.triple,
          "cargo",
          target_policy.profiling_argv(ctx.target.triple)?,
          ctx.root,
          environment,
        ),
      )
    }
    "test-health" => {
      guard args.is_empty() or args == ["--record"] else {
        return Err(usage("test-health accepts only --record"))
      }

      let records = tests.test_records(ctx)
      let native_events = fp"{records}/native-events.jsonl"
      guard native_events.exists() else {
        return Err(usage(f"no recorded full run at {records.display()}; run cargo dev test linux first"))
      }
      let cargo_records = [entry.path.display() for entry in fs.children(fp"{records}/cargo")? if entry.name.starts_with("cargo-")]
      var argv = ["python3", "-B", "dev/performance/rank.py", "--limit", "15"]
      if args == ["--record"] {
        let recorded = fp"{records}/commit"
        guard recorded.exists() else {
          return Err(usage(f"the run at {records.display()} did not record its commit"))
        }
        let day = time.format(time.now() * 1000000, "%Y-%m-%d", utc: true)?
        argv += ["--json", fp"{ctx.root}/bench/test-suite-health-{day}.json".display(), "--label", recorded.read_text()?.trim()]
      }
      return stages.execute(stages.command("test-health", ctx.target.triple, "python3",
        [@argv, native_events.display(), @cargo_records], ctx.root, {}))
    }
    "check" => {
      return builds.check_lint(ctx) when args == ["lint"]

      guard args.is_empty() else {
        return Err(usage("check accepts only the optional lint gate"))
      }

      return builds.check(ctx)
    }
    "gate" => {
      return gates.execute(ctx, args)
    }
    "docs" => {
      let tools = documentation.release_tools(ctx)
      return Err(usage("docs accepts only the optional check action")) unless args == [] or args == ["check"]

      documentation.build_release(ctx)
      return documentation.check(ctx.root, tools) when args == ["check"]

      return documentation.generate(ctx.root, tools)
    }
    "lint" => {
      let options = cli.parse(args, {fix: {form: "--fix", default: false}})?

      return Err(usage("lint is mutating and requires --fix")) unless options.fix

      return builds.lint_fix(ctx)
    }
    "test" => {
      let parsed = cli.parse(
        args,
        {
          kind: {
            form: "KIND",
            default: "rust",
          },
          ci: {
            form: "--ci",
            default: false,
          },
        },
      )?

      let options = TestOptions(test_kind(parsed.kind)?, parsed.ci)
      match options.kind {
        Rust => return tests.rust(ctx)
        Xsh => return tests.xsh(ctx)
        Linux => return tests.linux_test(ctx, options.ci)
        Macos => {
          guard options.ci else {
            return Err(usage("test macos is a CI-only target; pass --ci"))
          }

          return tests.macos_ci(ctx)
        }
      }
    }
    "coverage" => {
      let parsed = cli.parse(args, {backend: {form: "--backend BACKEND", default: ""}})?
      let options = CoverageOptions(parsed.backend)
      return coverage_workflow.coverage(ctx, coverage_workflow.parse_request(options.backend)?)
    }
    "bench" => {
      return benchmarks.benchmark(ctx, true) when args == ["--fast"]
      return benchmarks.syscalls(ctx) when args == ["--syscalls"]
      return benchmarks.pyramid(ctx, args)

    }
    "dist" => {
      let parsed = cli.parse(
        args,
        {
          docker: {
            form: "--docker POLICY",
            default: "auto",
          },
          ci: {
            form: "--ci",
            default: false,
          },
        },
      )?
      let options = DistOptions(...parsed)
      let docker_policy = distributions.parse_docker_policy(options.docker)?
      return distributions.build_distribution(ctx, docker_policy, options.ci)
    }
    "install" => {
      guard args.is_empty() else {
        return Err(usage("install accepts no arguments"))
      }

      return installations.install(ctx)
    }
    "release" => {
      let default_tag = env.get_or("RELEASE_TAG", "")?
      let parsed = cli.parse(
        args,
        {
          action: {
            form: "ACTION",
            required: true,
          },
          tag: {
            form: "--tag RELEASE-TAG",
            default: default_tag,
          },
        },
      )?

      let options: ReleaseOptions = parsed.require()?
      match release_operation(options.action)? {
        Smoke => return releases.smoke(ctx)
        Package => return releases.package_binaries(ctx, options.tag)
        Core => return releases.package_core(ctx, options.tag)
        Validate => return releases.validate_artifacts(ctx, options.tag)
      }
    }
    "internal" => {
      return Err(usage("internal requires an operation")) when args.is_empty()
      let operation = internal_operation(args[0])?
      let rest = args |> drop(1)
      guard rest.is_empty() or operation == GateLane else {
        return Err(usage("this internal operation accepts no extra arguments"))
      }

      match operation {
        Dist => return container_internal.container_dist(ctx)
        TestLinux => return container_internal.linux_developer_test(ctx)
        TestLinuxCi => return container_internal.linux_ci_test(ctx)
        TestLinuxSplit | TestLinuxCiSplit => return container_internal.linux_split_resource_test(ctx)
        GateLane => {
          return container_internal.gate_lane(ctx, rest)
        }
        GateFull => return container_internal.gate_full(ctx, args[0])
        Coverage => return container_internal.container_coverage(ctx)
      }
    }
    else => return Err(usage(f"unknown command {command}"))
  }
}

proc main(...raw: List[Str]) [fs, process, env, time, error, io] {
  if raw.is_empty() or raw[0] == "help" or raw[0] == "--help" or raw[0] == "-h" {
    print help_text()
    return
  }

  let command = raw[0]
  let global = parse_global(raw |> drop(1))?

  return dispatch(command, global.rest) when global.target == ""

  env ({TARGET: global.target}) {
    dispatch(command, global.rest)
  }
}

pure gate_request(arguments: List[Str]) -> Bool {
  ! arguments.is_empty() and (arguments[0] == "gate" or
    (arguments.len() > 1 and arguments[0] == "internal" and arguments[1] in ["gate-lane", "gate-full"]))
}

pure cancel_grace_ms(arguments: List[Str]) -> Int {
  if ! arguments.is_empty() and arguments[0] == "internal" { 20000 } else { 30000 }
}

on INT --pre-cancel=45s [fs, process, env, time, error, io] {
  if gate_request(args) {
    gates.cancel_owned_work(lifecycle.create()?, "INT", cancel_grace_ms(args))
    exit 130
  }
}
on TERM --pre-cancel=45s [fs, process, env, time, error, io] {
  if gate_request(args) {
    gates.cancel_owned_work(lifecycle.create()?, "TERM", cancel_grace_ms(args))
    exit 130
  }
}

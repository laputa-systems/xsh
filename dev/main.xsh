#!/usr/bin/env -S xsh --
# XSH owns the repository's development lifecycle. Cargo only bootstraps this
# script; each lifecycle operation below uses typed policy and direct argv.
use bench as benchmarks
use build as builds
use context as lifecycle
use coverage as coverage_workflow
use dist as distributions
use install as installations
use internal as container_internal
use release as releases
use test_workflows as tests

error DevUsage = Invalid(message: Str)

type GlobalOptions = {target: Str, rest: List[Str]}

type TestKind = Rust | Xsh | Linux | Macos

type TestOptions = {kind: TestKind, ci: Bool}

type CoverageOptions = {backend: Str}

type BenchOptions = {fast: Bool, syscalls: Bool}

type DistOptions = {docker: Str, ci: Bool}

type ReleaseOperation = Smoke | Package | Core | Validate

type ReleaseOptions = {action: Str, tag: Str}

pure test_kind(value: Str) -> Result[TestKind] {
  match value {
    "rust" => return Rust
    "xsh" => return Xsh
    "linux" => return Linux
    "macos" => return Macos
    _ => return Err(usage(f"unsupported test target ${value}"))
  }
}

pure release_operation(value: Str) -> Result[ReleaseOperation] {
  match value {
    "smoke" => return Smoke
    "package" => return Package
    "core" => return Core
    "validate" => return Validate
    _ => return Err(usage(f"unsupported release action ${value}"))
  }
}

type InternalOperation = Dist | TestLinux | TestLinuxCi | Coverage

pure internal_operation(value: Str) -> Result[InternalOperation] {
  match value {
    "dist" => return Dist
    "test-linux" => return TestLinux
    "test-linux-ci" => return TestLinuxCi
    "coverage" => return Coverage
    _ => return Err(usage(f"unsupported internal operation ${value}"))
  }
}

pure help_text() -> Str {
  return """XSH development lifecycle

usage: cargo dev COMMAND [OPTIONS]

commands:
  build
  check
  lint --fix
  test [xsh|linux|macos] [--ci]
  coverage [--backend native|docker]
  bench [--fast|--syscalls]
  dist [--target TRIPLE] [--docker auto|always|never] [--ci]
  install
  release smoke|package|core|validate [--tag RELEASE-TAG]

internal container commands are intentionally omitted from public help.
"""
}

pure usage(message: Str) -> Error {
  return DevUsage.Invalid(message: f"""${message}

${help_text()}""")
}

pure parse_global(args: List[Str]) -> Result[GlobalOptions] {
  var target = ""
  var rest: List[Str] = []
  var index = 0

  while index < args.len() {
    let arg = args[index]

    if arg == "--target" {
      if index + 1 >= args.len() {
        return Err(usage("--target requires a target triple"))
      }

      target = args[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--target=") {
      target = arg.split("=", maxsplit: 1).get(1, "")
      index += 1
      continue
    }

    rest = rest.push(arg)
    index += 1
  }

  return {target: target, rest: rest}
}

proc dispatch(command: Str, args: List[Str]) [fs, process, env, error, io] {
  let ctx = lifecycle.create()?

  match command {
    "build" => {
      if args.len() != 0 {
        return Err(usage("build accepts no arguments"))
      }

      return builds.build(ctx)
    }
    "check" => {
      if args.len() != 0 {
        return Err(usage("check accepts no arguments"))
      }

      return builds.check(ctx)
    }
    "lint" => {
      let options = cli.parse(args, {fix: {form: "--fix", default: false}})?

      if ! options.fix {
        return Err(usage("lint is mutating and requires --fix"))
      }

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

      let options: TestOptions = {kind: test_kind(parsed.kind)?, ci: parsed.ci}
      match options.kind {
        Rust => return tests.rust(ctx)
        Xsh => return tests.xsh(ctx)
        Linux => return tests.linux_test(ctx, options.ci)
        Macos => {
          if ! options.ci {
            return Err(usage("test macos is a CI-only target; pass --ci"))
          }

          return tests.macos_ci(ctx)
        }
      }
    }
    "coverage" => {
      let parsed = cli.parse(args, {backend: {form: "--backend BACKEND", default: ""}})?
      let options: CoverageOptions = {backend: parsed.backend}
      return coverage_workflow.coverage(ctx, coverage_workflow.parse_request(options.backend)?)
    }
    "bench" => {
      let options: BenchOptions = cli.parse(
        args,
        {
          fast: {
            form: "--fast",
            default: false,
            conflicts: "syscalls",
          },
          syscalls: {
            form: "--syscalls",
            default: false,
            conflicts: "fast",
          },
        },
      )?

      if options.syscalls {
        return benchmarks.syscalls(ctx)
      }

      return benchmarks.benchmark(ctx, options.fast)
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
      let options: DistOptions = {docker: parsed.docker, ci: parsed.ci}
      let docker_policy = distributions.parse_docker_policy(options.docker)?
      return distributions.build_distribution(ctx, docker_policy, options.ci)
    }
    "install" => {
      if args.len() != 0 {
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

      let options: ReleaseOptions = {action: parsed.action, tag: parsed.tag}
      match release_operation(options.action)? {
        Smoke => return releases.smoke(ctx)
        Package => return releases.package_binaries(ctx, options.tag)
        Core => return releases.package_core(ctx, options.tag)
        Validate => return releases.validate_artifacts(ctx, options.tag)
      }
    }
    "internal" => {
      let operation = cli.parse(args, {operation: {form: "OPERATION", required: true}})?.operation

      match internal_operation(operation)? {
        Dist => return container_internal.container_dist(ctx)
        TestLinux => return container_internal.linux_developer_test(ctx)
        TestLinuxCi => return container_internal.linux_ci_test(ctx)
        Coverage => return container_internal.container_coverage(ctx)
      }
    }
    _ => return Err(usage(f"unknown command ${command}"))
  }
}

proc main(...raw: List[Str]) [fs, process, env, error, io] {
  if raw.len() == 0 or raw[0] == "help" or raw[0] == "--help" or raw[0] == "-h" {
    print help_text()
    return
  }

  let command = raw[0]
  let global = parse_global(raw |> drop(1))?

  if global.target == "" {
    return dispatch(command, global.rest)
  }

  env TARGET=global.target {
    dispatch(command, global.rest)?
  } ?
}

main(@args)?

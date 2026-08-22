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

type TestOptions = {kind: Str, ci: Bool}

type CoverageOptions = {backend: Str}

type BenchOptions = {fast: Bool, syscalls: Bool}

type DistOptions = {docker: Str, ci: Bool}

type ReleaseOptions = {action: Str, tag: Str}

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
      let options: TestOptions = cli.parse(
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

      match options.kind {
        "rust" => return tests.rust(ctx)
        "xsh" => return tests.xsh(ctx)
        "linux" => return tests.linux_test(ctx, options.ci)
        "macos" => {
          if ! options.ci {
            return Err(usage("test macos is a CI-only target; pass --ci"))
          }

          return tests.macos_ci(ctx)
        }
        _ => return Err(usage(f"unsupported test target ${options.kind}"))
      }
    }
    "coverage" => {
      let options: CoverageOptions = cli.parse(args, {backend: {form: "--backend BACKEND", default: ""}})?
      return coverage_workflow.coverage(ctx, options.backend)
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
      let options: DistOptions = cli.parse(
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
      return distributions.build_distribution(ctx, options.docker, options.ci)
    }
    "install" => {
      if args.len() != 0 {
        return Err(usage("install accepts no arguments"))
      }

      return installations.install(ctx)
    }
    "release" => {
      let default_tag = env.get_or("RELEASE_TAG", "")?
      let options: ReleaseOptions = cli.parse(
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

      match options.action {
        "smoke" => return releases.smoke(ctx)
        "package" => return releases.package_binaries(ctx, options.tag)
        "core" => return releases.package_core(ctx, options.tag)
        "validate" => return releases.validate_artifacts(ctx, options.tag)
        _ => return Err(usage(f"unsupported release action ${options.action}"))
      }
    }
    "internal" => {
      let operation = cli.parse(args, {operation: {form: "OPERATION", required: true}})?.operation

      match operation {
        "dist" => return container_internal.container_dist(ctx)
        "test-linux" => return container_internal.linux_developer_test(ctx)
        "test-linux-ci" => return container_internal.linux_ci_test(ctx)
        "coverage" => return container_internal.container_coverage(ctx)
        _ => return Err(usage(f"unsupported internal operation ${operation}"))
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

#!/usr/bin/env -S xsh --
# XSH owns the repository's development lifecycle. Cargo only bootstraps this
# script; each lifecycle operation below uses typed policy and direct argv.
use bench as benchmarks
use build as builds
use context as lifecycle
use coverage as coverage_workflow
use dist as distributions
use docs as documentation
use install as installations
use internal as container_internal
use release as releases
use system_report_check as system_reports
use test_workflows as tests

type GlobalOptions = {target: Str, rest: List[Str]}

enum TestKind {
    Rust,
    Xsh,
    Linux,
    Macos,
}

type TestOptions = {kind: TestKind, ci: Bool}

type CoverageOptions = {backend: Str}

type BenchOptions = {fast: Bool, syscalls: Bool}

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
    Coverage,
}

pure internal_operation(value: Str) -> Result[InternalOperation] {
  match value {
    "dist" => Dist
    "test-linux" => TestLinux
    "test-linux-ci" => TestLinuxCi
    "coverage" => Coverage
    else => Err(usage(f"unsupported internal operation {value}"))
  }
}

pure help_text() -> Str {
  """XSH development lifecycle

usage: cargo dev COMMAND [OPTIONS]

commands:
  build
  check [lint]
  docs [check]
  lint --fix
  test [xsh|linux|macos] [--ci]
  coverage [--backend native|docker]
  bench [--fast|--syscalls]
  dist [--target TRIPLE] [--docker auto|always|never] [--ci]
  install
  release smoke|package|core|validate [--tag RELEASE-TAG]
  system-report-check [--manifest FILE] [--run-fixtures --xsh-bin FILE --xsht-bin FILE --cargo-bin FILE]
                      [--run-macos-fixtures --xsh-bin FILE --xsht-bin FILE]
                      [--capture-cpu-bundle NEW_DIRECTORY | --replay-cpu-bundle DIRECTORY]
                      [--capture-cpufreq-bundle NEW_DIRECTORY | --replay-cpufreq-bundle DIRECTORY]
                      [--capture-cpu-topology-bundle NEW_DIRECTORY | --replay-cpu-topology-bundle DIRECTORY]
                      [--capture-memory-bundle NEW_DIRECTORY | --replay-memory-bundle DIRECTORY]
                      [--capture-pressure-bundle NEW_DIRECTORY | --replay-pressure-bundle DIRECTORY]
                      [--capture-swaps-bundle NEW_DIRECTORY | --replay-swaps-bundle DIRECTORY]
                      [--capture-os-release-bundle NEW_DIRECTORY | --replay-os-release-bundle DIRECTORY]
                      [--capture-uptime-bundle NEW_DIRECTORY | --replay-uptime-bundle DIRECTORY]
                      [--capture-dmi-identity-bundle NEW_DIRECTORY | --replay-dmi-identity-bundle DIRECTORY]
                      [--capture-device-tree-bundle NEW_DIRECTORY | --replay-device-tree-bundle DIRECTORY]
                      [--capture-kernel-command-line-bundle NEW_DIRECTORY | --replay-kernel-command-line-bundle DIRECTORY]
                      [--capture-kernel-modules-bundle NEW_DIRECTORY | --replay-kernel-modules-bundle DIRECTORY]
                      [--capture-vulnerabilities-bundle NEW_DIRECTORY | --replay-vulnerabilities-bundle DIRECTORY]
                      [--capture-mountinfo-bundle NEW_DIRECTORY | --replay-mountinfo-bundle DIRECTORY]
                      [--capture-kernel-parameters-bundle NEW_DIRECTORY | --replay-kernel-parameters-bundle DIRECTORY]
                      [--capture-thermal-bundle NEW_DIRECTORY | --replay-thermal-bundle DIRECTORY]
                      [--capture-hwmon-bundle NEW_DIRECTORY | --replay-hwmon-bundle DIRECTORY]
                      [--capture-block-bundle NEW_DIRECTORY | --replay-block-bundle DIRECTORY]
                      [--capture-cgroup2-bundle NEW_DIRECTORY | --replay-cgroup2-bundle DIRECTORY]
                      [--capture-process-bundle NEW_DIRECTORY | --replay-process-bundle DIRECTORY]
                      [--capture-power-supply-bundle NEW_DIRECTORY | --replay-power-supply-bundle DIRECTORY]
                      [--capture-powercap-bundle NEW_DIRECTORY | --replay-powercap-bundle DIRECTORY]
                      [--capture-pci-bundle NEW_DIRECTORY | --replay-pci-bundle DIRECTORY]
                      [--capture-usb-bundle NEW_DIRECTORY | --replay-usb-bundle DIRECTORY]
                      [--capture-smbios-bundle NEW_DIRECTORY | --replay-smbios-bundle DIRECTORY]
                      [--corroborate-smbios-bundle DIRECTORY --dmidecode-bin FILE]
                      [--compare-cpu] [--compare-cpu-topology] [--compare-cpu-cache]
                      [--compare-cpufreq] [--compare-cpuidle]
                      [--compare-cpupower --cpupower-bin FILE]
                      [--compare-cpu-scope] [--compare-cgroup-v2]
                      [--compare-vulnerabilities] [--compare-huge-pages]
                      [--compare-pressure]
                      [--compare-swaps] [--compare-storage] [--compare-queue]
                      [--compare-mounts] [--compare-mount-usage] [--compare-modules]
                      [--compare-command-line] [--compare-parameters]
                      [--compare-identity] [--compare-namespaces]
                      [--compare-pci] [--compare-pci-bindings] [--compare-pci-links]
                      [--compare-usb-topology] [--compare-usb-ids] [--compare-usb-power]
                      [--compare-usb-interfaces]
                      [--compare-lsusb --lsusb-bin FILE]
                      [--compare-power-supplies]
                      [--compare-powercap]
                      [--compare-device-classes]
                      [--compare-hwmon]
                      [--compare-sensors-json --sensors-bin FILE]
                      [--compare-smbios]
                      [--compare-thermal]
                      [--no-subprocess --xsh-bin FILE --script FILE]

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
    "build" => {
      guard args.is_empty() else {
        return Err(usage("build accepts no arguments"))
      }

      return builds.build(ctx)
    }
    "check" => {
      return builds.check_lint(ctx) when args == ["lint"]

      guard args.is_empty() else {
        return Err(usage("check accepts only the optional lint gate"))
      }

      return builds.check(ctx)
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

      return benchmarks.syscalls(ctx) when options.syscalls

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
    "system-report-check" => return system_reports.validate_and_run(ctx, args)
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
      let operation = cli.parse(args, {operation: {form: "OPERATION", required: true}})?.operation

      match internal_operation(operation)? {
        Dist => return container_internal.container_dist(ctx)
        TestLinux => return container_internal.linux_developer_test(ctx)
        TestLinuxCi => return container_internal.linux_ci_test(ctx)
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

  env TARGET=$global.target {
    dispatch(command, global.rest)
  }
}

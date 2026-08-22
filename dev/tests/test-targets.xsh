use context as lifecycle
use coverage as coverage_workflow
use docker as docker_workflows
use release as releases
use targets as target_policy

proc test_supported_target_records_and_default() [error] {
  test.eq(target_policy.default_triple, "x86_64-unknown-linux-musl")?
  let x86 = target_policy.resolve("x86_64-unknown-linux-musl")?
  let arm = target_policy.resolve("aarch64-unknown-linux-musl")?
  let darwin = target_policy.resolve("aarch64-apple-darwin")?
  test.eq(x86.docker_platform, "linux/amd64")?
  test.eq(x86.elf_machine, "Advanced Micro Devices X86-64")?
  test.eq(arm.docker_platform, "linux/arm64")?
  test.eq(arm.elf_machine, "AArch64")?
  test.eq(darwin.executable_format, "Mach-O")?
  test.ok("target-cpu=apple-m1" in darwin.cpu_rustflags)?
}

proc test_host_classification() [error] {
  test.eq(target_policy.host_os("Linux")?, "linux")?
  test.eq(target_policy.host_os("Darwin")?, "darwin")?
  test.eq(target_policy.host_arch("amd64")?, "x86_64")?
  test.eq(target_policy.host_arch("arm64")?, "aarch64")?
  match target_policy.host_arch("mips64") {
    Ok(_) => test.fail("unsupported host architecture resolved")?
    Err(error) => test.eq(error.message, "TargetError.Unsupported")?
  }
}

proc test_target_flags_native_selection_and_coverage_backend_policy() [error] {
  let x86_env = target_policy.distribution_env("x86_64-unknown-linux-musl", "-C debuginfo=1", "-O2", "26.0")?
  let arm_env = target_policy.distribution_env("aarch64-unknown-linux-musl", "", "", "26.0")?
  let darwin_env = target_policy.distribution_env("aarch64-apple-darwin", "-C debuginfo=1", "", "27.0")?
  test.contains(x86_env.CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS, "-C debuginfo=1")?
  test.contains(x86_env.CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS, "target-cpu=x86-64-v3")?
  test.contains(x86_env.CFLAGS_x86_64_unknown_linux_musl, "-march=x86-64-v3")?
  test.contains(arm_env.CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS, "target-feature=-sve,-sve2")?
  test.eq(darwin_env.MACOSX_DEPLOYMENT_TARGET, "27.0")?
  test.contains(darwin_env.CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS, "linker-flavor=ld64.lld")?
  test.ok(target_policy.native_execution("x86_64-unknown-linux-musl", "linux", "x86_64")?)?
  test.ok(! target_policy.native_execution("x86_64-unknown-linux-musl", "darwin", "x86_64")?)?
  test.ok(! target_policy.native_execution("aarch64-apple-darwin", "linux", "aarch64")?)?
  let alpine_x86 = {
    root: /repo,
    target_dir: /repo/target,
    coverage_dir: /repo/target/cov,
    artifact_dir: /repo/dist,
    host_os: "linux",
    host_arch: "x86_64",
    target: {
      triple: "x86_64-unknown-linux-musl",
      os: "linux",
      arch: "x86_64",
      docker_platform: "linux/amd64",
      executable_format: "ELF",
      elf_machine: "Advanced Micro Devices X86-64",
      cpu_rustflags: [],
      cpu_cflags: [],
      static_musl: true,
    },
    profile: "dist",
    darwin_deployment_target: "26.0",
  }
  test.eq(coverage_workflow.automatic_backend_for(alpine_x86, true, true, true), "native")?
  test.eq(coverage_workflow.automatic_backend_for(alpine_x86, false, true, true), "docker")?
  test.eq(coverage_workflow.automatic_backend_for(alpine_x86, true, false, true), "docker")?
  test.eq(coverage_workflow.automatic_backend_for(alpine_x86, true, true, false), "docker")?
}

proc test_docker_argv_is_direct_and_carries_mount_environment_policy() [error] {
  let ctx = {
    root: /repo,
    target_dir: /repo/target,
    coverage_dir: /repo/target/cov,
    artifact_dir: /repo/dist,
    host_os: "darwin",
    host_arch: "aarch64",
    target: {
      triple: "aarch64-unknown-linux-musl",
      os: "linux",
      arch: "aarch64",
      docker_platform: "linux/arm64",
      executable_format: "ELF",
      elf_machine: "AArch64",
      cpu_rustflags: [
        "-C",
        "target-cpu=neoverse-n2",
      ],
      cpu_cflags: [
        "-mcpu=neoverse-n2+nosve+nosve2",
      ],
      static_musl: true,
    },
    profile: "dist",
    darwin_deployment_target: "26.0",
  }
  let argv = docker_workflows.internal_argv(ctx, "xsh-test", "linux/arm64", "dist", true, 501, 20, "25", [])
  test.eq(argv[0], "docker")?
  test.ok("--privileged" in argv)?
  test.ok("/repo:/work" in argv)?
  test.ok("/repo/target:/work/target" in argv)?
  test.ok("TARGET=aarch64-unknown-linux-musl" in argv)?
  test.ok("XSH_OS_STRESS_REPEAT=25" in argv)?
  test.ok("dev/main.xsh" in argv)?
  test.ok("sh" not in argv)?
  test.ok("-c" not in argv)?
}

proc test_release_names_and_core_paths_are_deterministic() [error] {
  test.eq(target_policy.release_suffix("x86_64-unknown-linux-musl")?, "x86_64-linux-musl")?
  test.eq(target_policy.release_suffix("aarch64-unknown-linux-musl")?, "aarch64-linux-musl")?
  test.eq(target_policy.release_suffix("aarch64-apple-darwin")?, "aarch64-apple-darwin")?
  test.eq(releases.core_install_path(p"bin/hello.xsh").display(), "core/bin/hello")?
  test.eq(releases.core_install_path(p"top.xsh").display(), "core/top")?
}

proc test_release_checksum_sidecars_keep_a_relative_artifact_name(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "release-checksum")?
  let artifact = fp"${root}/dist/xsh-release-x86_64-linux-musl"
  artifact.parent().mkdir()?
  artifact.write("release artifact")?
  let checksum = releases.checksum_line(artifact, root)?
  test.contains(
    checksum,
    """  dist/xsh-release-x86_64-linux-musl
""",
    checksum,
  )?
}

proc test_release_validation_requires_exactly_the_nine_expected_products(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "release-validation")?
  let artifact_dir = fp"${root}/dist"
  artifact_dir.mkdir()?
  let release_ctx = {
    root: root,
    target_dir: fp"${root}/target",
    coverage_dir: fp"${root}/target/cov",
    artifact_dir: artifact_dir,
    host_os: "linux",
    host_arch: "x86_64",
    target: {
      triple: "x86_64-unknown-linux-musl",
      os: "linux",
      arch: "x86_64",
      docker_platform: "linux/amd64",
      executable_format: "ELF",
      elf_machine: "Advanced Micro Devices X86-64",
      cpu_rustflags: [],
      cpu_cflags: [],
      static_musl: true,
    },
    profile: "dist",
    darwin_deployment_target: "26.0",
  }
  let tag = "release-test"

  for triple in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl", "aarch64-apple-darwin"] {
    let suffix = target_policy.release_suffix(triple)?

    for product in target_policy.products {
      let artifact = fp"${artifact_dir}/${product}-${tag}-${suffix}"
      artifact.write("release artifact")?
      fs.chmod(artifact, 0o755)?
      fp"${artifact.display()}.sha256".write(releases.checksum_line(artifact, root)?)?
    }
  }

  releases.validate_artifacts(release_ctx, tag)?
  fp"${artifact_dir}/unexpected-file".write("not a release artifact")?

  match releases.validate_artifacts(release_ctx, tag) {
    Ok(_) => test.fail("unexpected artifact passed validation")?
    Err(error) => test.contains(error.message, "ContextError.StageFailed", error.message)?
  }
}

proc test_unsupported_target_remains_a_structured_error() [error] {
  match target_policy.resolve("riscv64-unknown-linux-musl") {
    Ok(_) => test.fail("unsupported target resolved")?
    Err(error) => test.eq(error.message, "TargetError.Unsupported")?
  }
}

proc test_context_paths_and_missing_tools_have_named_failures() [process, error] {
  test.eq(lifecycle.repo_path(/repo, "target/custom").display(), "/repo/target/custom")?
  test.eq(lifecycle.repo_path(/repo, "/tmp/custom").display(), "/tmp/custom")?

  match lifecycle.require_tool("xsh-selfhost-test-tool-that-does-not-exist") {
    Ok(_) => test.fail("missing tool unexpectedly resolved")?
    Err(error) => test.eq(error.message, "ContextError.MissingTool")?
  }
}

proc test_failed_stage_reports_its_stage_and_target() [process, error, io] {
  match lifecycle.run_stage("selfhost-stage", "selfhost-target", "false", ["false"], p".", {}) {
    Ok(_) => test.fail("failing command unexpectedly succeeded")?
    Err(error) => test.eq(error.message, "ContextError.StageFailed")?
  }
}

proc test_subprocess_wrong_directory_has_a_named_failure(ctx: TestContext) [fs, error] {
  let repository = fs.cwd()?
  let module_path = fp"${repository}/dev".display()
  let wrong_directory = test.run_script(
    ctx,
    """
use context

cd p"/" {
  match context.require_root() {
    Ok(_) => abort(1)
    Err(error) => print \${error.message}
  }
}?
""",
    [],
    {XSH_MODULE_PATH: module_path},
  )?
  test.ok(
    wrong_directory.success,
    f"""${wrong_directory.stdout}
${wrong_directory.stderr}""",
  )?
  test.contains(wrong_directory.stdout, "ContextError.WrongDirectory", wrong_directory.stdout)?
}

proc test_context_target_and_docker_platform_overrides(ctx: TestContext) [fs, error] {
  let repository = fs.cwd()?
  let module_path = fp"${repository}/dev".display()
  let context_default = test.run_script(
    ctx,
    """
use context

proc main() [fs, env, error] -> Result[Unit] {
  print \${(context.create()?).target.triple}
}

main()?
""",
    [],
    {XSH_MODULE_PATH: module_path, TARGET: ""},
  )?
  let context_override = test.run_script(
    ctx,
    """
use context

proc main() [fs, env, error] -> Result[Unit] {
  print \${(context.create()?).target.triple}
}

main()?
""",
    [],
    {XSH_MODULE_PATH: module_path, TARGET: "aarch64-unknown-linux-musl"},
  )?
  test.ok(context_default.success, context_default.stderr)?
  test.ok(context_override.success, context_override.stderr)?
  test.eq(context_default.stdout.trim(), target_policy.default_triple)?
  test.eq(context_override.stdout.trim(), "aarch64-unknown-linux-musl")?

  let platform = test.run_script(
    ctx,
    """
use docker

let ctx = {
  root: p"/repo",
  target_dir: p"/repo/target",
  coverage_dir: p"/repo/target/cov",
  artifact_dir: p"/repo/dist",
  host_os: "linux",
  host_arch: "x86_64",
  target: {
    triple: "x86_64-unknown-linux-musl",
    os: "linux",
    arch: "x86_64",
    docker_platform: "linux/amd64",
    executable_format: "ELF",
    elf_machine: "Advanced Micro Devices X86-64",
    cpu_rustflags: [],
    cpu_cflags: [],
    static_musl: true,
  },
  profile: "dist",
  darwin_deployment_target: "26.0",
}
proc main() [env, error] -> Result[Unit] {
  print \${docker.platform(ctx)?}
}

main()?
""",
    [],
    {XSH_MODULE_PATH: module_path, DOCKER_PLATFORM: "linux/override"},
  )?
  test.ok(platform.success, platform.stderr)?
  test.eq(platform.stdout.trim(), "linux/override")?
}

proc test_rustybench_override_stays_a_direct_argv_prefix(ctx: TestContext) [fs, error] {
  let repository = fs.cwd()?
  let module_path = fp"${repository}/dev".display()

  let rustybench = test.run_script(
    ctx,
    """
use bench

let ctx = {
  root: p"/repo",
  target_dir: p"/repo/target",
  coverage_dir: p"/repo/target/cov",
  artifact_dir: p"/repo/dist",
  host_os: "linux",
  host_arch: "x86_64",
  target: {
    triple: "x86_64-unknown-linux-musl",
    os: "linux",
    arch: "x86_64",
    docker_platform: "linux/amd64",
    executable_format: "ELF",
    elf_machine: "Advanced Micro Devices X86-64",
    cpu_rustflags: [],
    cpu_cflags: [],
    static_musl: true,
  },
  profile: "dist",
  darwin_deployment_target: "26.0",
}
print \${(bench.command_prefix(ctx)?).join("|")}
""",
    [],
    {
      XSH_MODULE_PATH: module_path,
      RUSTYBENCH: "cargo run --quiet --manifest-path /tmp/rustybench/Cargo.toml --",
    },
  )?
  test.ok(
    rustybench.success,
    f"""${rustybench.stdout}
${rustybench.stderr}""",
  )?
  test.eq(
    rustybench.stdout.trim(),
    "cargo|run|--quiet|--manifest-path|/tmp/rustybench/Cargo.toml|--",
    rustybench.stdout,
  )?
}

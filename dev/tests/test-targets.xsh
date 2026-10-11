use context as lifecycle
use coverage as coverage_workflow
use docker as docker_workflows
use fixtures
use release as releases
use stage as stages
use targets as target_policy

test test_supported_target_records_and_default {
  assert target_policy.default_triple == "x86_64-unknown-linux-musl"
  assert target_policy.host_default_triple(target_policy.Linux, target_policy.X86_64)? == "x86_64-unknown-linux-musl"
  assert target_policy.host_default_triple(target_policy.Linux, target_policy.Aarch64)? == "aarch64-unknown-linux-musl"
  assert target_policy.host_default_triple(target_policy.Darwin, target_policy.Aarch64)? == "aarch64-apple-darwin"
  match target_policy.host_default_triple(target_policy.Darwin, target_policy.X86_64) {
    Ok(_) => test.fail("unsupported host default resolved")
    Err(error) => assert error.message == "TargetError.Unsupported"
  }

  let x86 = target_policy.resolve("x86_64-unknown-linux-musl")?
  let arm = target_policy.resolve("aarch64-unknown-linux-musl")?
  let darwin = target_policy.resolve("aarch64-apple-darwin")?
  assert x86.docker_platform == "linux/amd64"
  assert x86.elf_machine == "Advanced Micro Devices X86-64"
  assert arm.docker_platform == "linux/arm64"
  assert arm.elf_machine == "AArch64"
  assert darwin.executable_format == "Mach-O"
  assert "target-cpu=apple-m1" in darwin.cpu_rustflags
}

test test_host_classification {
  assert target_policy.host_os("Linux")? == "linux"
  assert target_policy.host_os("Darwin")? == "darwin"
  assert target_policy.host_arch("amd64")? == "x86_64"
  assert target_policy.host_arch("arm64")? == "aarch64"
  match target_policy.host_arch("mips64") {
    Ok(_) => test.fail("unsupported host architecture resolved")
    Err(error) => assert error.message == "TargetError.Unsupported"
  }
}

test test_target_flags_native_selection_and_coverage_backend_policy {
  let x86_env = target_policy.distribution_env(
    "x86_64-unknown-linux-musl",
    "-C debuginfo=1",
    "-O2",
    "26.0",
  )?
  let arm_env = target_policy.distribution_env("aarch64-unknown-linux-musl", "", "", "26.0")?
  let darwin_env = target_policy.distribution_env(
    "aarch64-apple-darwin",
    "-C debuginfo=1",
    "",
    "27.0",
  )?
  let x86_target = target_policy.resolve("x86_64-unknown-linux-musl")?
  let darwin_target = target_policy.resolve("aarch64-apple-darwin")?
  assert "-C debuginfo=1" in x86_env.CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS.require(Str)?
  assert "target-cpu=x86-64-v3" in x86_env.CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS.require(Str)?
  assert "-march=x86-64-v3" in x86_env.CFLAGS_x86_64_unknown_linux_musl.require(Str)?
  assert "target-feature=-sve,-sve2" in arm_env.CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS.require(Str)?
  assert darwin_env.MACOSX_DEPLOYMENT_TARGET == "27.0"
  assert "linker-flavor=ld64.lld" in darwin_env.CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS.require(Str)?
  assert target_policy.native_execution(x86_target, target_policy.Linux, target_policy.X86_64)
  assert ! target_policy.native_execution(x86_target, target_policy.Darwin, target_policy.X86_64)
  assert ! target_policy.native_execution(darwin_target, target_policy.Linux, target_policy.Aarch64)
  assert coverage_workflow.backend_name(coverage_workflow.automatic_backend()) == "docker"
  assert coverage_workflow.docker_target_triple(target_policy.Aarch64, "") == "aarch64-unknown-linux-musl"
  assert coverage_workflow.docker_target_triple(target_policy.X86_64, "") == "x86_64-unknown-linux-musl"
  assert coverage_workflow.docker_target_triple(target_policy.Aarch64, "x86_64-unknown-linux-musl") == "x86_64-unknown-linux-musl"
}

test test_docker_argv_is_direct_and_carries_mount_environment_policy {
  let ctx = fixtures.linux_aarch64_context(/repo, "dev")?
  let argv = docker_workflows.internal_argv(
    ctx,
    "xsh-test",
    "linux/arm64",
    "dist",
    true,
    501,
    20,
    "25",
    [],
  )?
  assert argv[0] == "docker"
  assert "--init" in argv
  assert "--privileged" in argv
  assert "/repo:/work" in argv
  assert "/repo/target:/work/target" in argv
  assert "TARGET=aarch64-unknown-linux-musl" in argv
  assert "DIST_PROFILE=dev" in argv
  assert "XSH_OS_STRESS_REPEAT=25" in argv
  assert "dev/main.xsh" in argv
  assert "sh" not in argv
  assert "-c" not in argv
}

test test_release_names_and_core_paths_are_deterministic {
  assert target_policy.release_suffix("x86_64-unknown-linux-musl")? == "x86_64-linux-musl"
  assert target_policy.release_suffix("aarch64-unknown-linux-musl")? == "aarch64-linux-musl"
  assert target_policy.release_suffix("aarch64-apple-darwin")? == "aarch64-apple-darwin"
  assert releases.core_install_path(p"bin/hello.xsh") == "core/bin/hello"
  assert releases.core_install_path(p"top.xsh") == "core/top"
  assert releases.core_install_path(p"system-report.xsh") == "core/system-report"
  assert releases.core_install_path(p"bin/report.xsh-helper.xsh") == "core/bin/report.xsh-helper"
  assert releases.core_install_path(p"lib/auth.xsh") == "core/lib/auth.xsh"
}

test test_core_archive_stages_command_and_library_paths { |ctx|
  let root = test.temp_dir(ctx, name: "core-archive")?
  fp"{root}/core".mkdir()
  fp"{root}/core/bin".mkdir()
  fp"{root}/core/lib".mkdir()
  fp"{root}/core/tests".mkdir()
  fp"{root}/core/bin/report.xsh-helper.xsh".write("""print "command"
""")
  fp"{root}/core/lib/report.xsh-helper.xsh".write("""print "library"
""")
  fp"{root}/core/tests/ignored.xsh".write("""print "test"
""")
  let release_ctx = fixtures.linux_context(root, "dev")?
  releases.package_core(release_ctx, "fixture")
  let archive_path = fp"{root}/dist/core-fixture.tar.xz"
  let entries = archive.tar_list(archive_path)?.collect()
  let members = entries |> map .path.display()
  assert members.len() == 3
  assert "core/bin/report.xsh-helper" in members
  assert "core/lib/report.xsh-helper.xsh" in members
  assert "core/applets.json" in members
  assert (entries |> where .path == "core/bin/report.xsh-helper")[0].mode.bit_and(0o777) == 0o755
  assert (entries |> where .path == "core/lib/report.xsh-helper.xsh")[0].mode.bit_and(0o777) == 0o644
  assert (entries |> where .path == "core/applets.json")[0].mode.bit_and(0o777) == 0o644
  let extracted = fp"{root}/extracted"
  archive.tar_extract(archive_path, extracted)
  assert fp"{extracted}/core/bin/report.xsh-helper".read_text()? == """print "command"
"""
  assert fp"{extracted}/core/lib/report.xsh-helper.xsh".read_text()? == """print "library"
"""
  assert ! fp"{extracted}/core/tests/ignored".exists()?
  assert """  dist/core-fixture.tar.xz
""" in fp"{root}/dist/core-fixture.sha256".read_text()?
}

type ManifestApplet = {name: Str, source: Str}

type ManifestAlias = {name: Str, target: Str}

type ManifestShape = {
  aliases: List[ManifestAlias],
  applets: List[ManifestApplet],
  generated_by: Str,
  libraries: List[Str],
}

test test_applet_manifest_lists_applets_aliases_and_libraries { |ctx|
  let root = test.temp_dir(ctx, name: "applet-manifest")?
  fp"{root}/core/lib".mkdir()
  fp"{root}/core/tests".mkdir()
  fp"{root}/dev/compat".mkdir()
  fp"{root}/core/ls.xsh".write("print \"ls\"\n")
  fp"{root}/core/basename.xsh".write("print \"basename\"\n")
  fp"{root}/core/lib/gnu.xsh".write("##! gnu\n")
  fp"{root}/core/tests/test-ls.xsh".write("print \"test\"\n")
  fp"{root}/dev/compat/aliases.json".write(
    """{"comment": "x", "aliases": [{"name": "vdir", "target": "ls"}, {"name": "dir", "target": "ls"}]}\n""",
  )
  let release_ctx = fixtures.linux_context(root, "dev")?
  let text = releases.applet_manifest(release_ctx)?
  assert text.ends_with("\n")
  let manifest = json.decode(text)?.require(ManifestShape)?
  assert manifest.generated_by == "dev/release.xsh"
  assert (manifest.applets |> map .name) == ["basename", "ls"]
  assert (manifest.applets |> map .source) == ["core/basename.xsh", "core/ls.xsh"]
  assert (manifest.aliases |> map .name) == ["dir", "vdir"]
  assert (manifest.aliases |> map .target) == ["ls", "ls"]
  assert manifest.libraries == ["lib/gnu.xsh"]
  assert releases.applet_manifest(release_ctx)? == text
}

test test_applet_manifest_without_alias_table_has_no_aliases { |ctx|
  let root = test.temp_dir(ctx, name: "applet-manifest-no-aliases")?
  fp"{root}/core".mkdir()
  fp"{root}/core/cat.xsh".write("print \"cat\"\n")
  let release_ctx = fixtures.linux_context(root, "dev")?
  let manifest = json.decode(releases.applet_manifest(release_ctx)?)?.require(ManifestShape)?
  assert manifest.aliases.is_empty()
  assert (manifest.applets |> map .name) == ["cat"]
}

test test_applet_manifest_rejects_dangling_and_colliding_aliases { |ctx|
  let root = test.temp_dir(ctx, name: "applet-manifest-bad-aliases")?
  fp"{root}/core".mkdir()
  fp"{root}/dev/compat".mkdir()
  fp"{root}/core/ls.xsh".write("print \"ls\"\n")
  fp"{root}/core/cat.xsh".write("print \"cat\"\n")
  let release_ctx = fixtures.linux_context(root, "dev")?

  fp"{root}/dev/compat/aliases.json".write("""{"aliases": [{"name": "dir", "target": "missing"}]}""")
  match releases.applet_manifest(release_ctx) {
    Ok(_) => test.fail("dangling alias was accepted")
    Err(stages.StageError.Failed {detail, ..}) => assert detail == "alias dir targets missing applet missing"
    Err(error) => test.fail(error.message)
  }

  fp"{root}/dev/compat/aliases.json".write("""{"aliases": [{"name": "cat", "target": "ls"}]}""")
  match releases.applet_manifest(release_ctx) {
    Ok(_) => test.fail("colliding alias was accepted")
    Err(stages.StageError.Failed {detail, ..}) => assert detail == "alias cat collides with applet core/cat.xsh"
    Err(error) => test.fail(error.message)
  }
}

test test_core_archive_rejects_conflicting_artifact_before_writing { |ctx|
  let root = test.temp_dir(ctx, name: "core-archive-conflict")?
  fp"{root}/core".mkdir()
  fp"{root}/core/report.xsh".write("""print "ok"
""")
  fp"{root}/dist".mkdir()
  let stale = fp"{root}/dist/unrelated.tar.xz"
  stale.write("existing artifact")
  let release_ctx = fixtures.linux_context(root, "dev")?
  match releases.package_core(release_ctx, "fixture") {
    Ok(_) => test.fail("conflicting archive was accepted")
    Err(error) => assert "StageError.Failed" in error.message
  }

  assert stale.read_text()? == "existing artifact"
  assert ! fp"{root}/dist/core-fixture.tar.xz".exists()?
  assert ! fp"{root}/dist/core-fixture.sha256".exists()?
}

test test_core_archive_contains_current_system_report { |ctx|
  let repository = ctx.core_dir.parent()
  let artifact_dir = test.temp_dir(ctx, name: "system-report-core-archive")?
  let base = fixtures.linux_context(repository, "dev")?
  let release_ctx = lifecycle.Context(
    root: base.root,
    target_dir: base.target_dir,
    coverage_dir: base.coverage_dir,
    artifact_dir:,
    host_os: base.host_os,
    host_arch: base.host_arch,
    target: base.target,
    profile: base.profile,
    darwin_deployment_target: base.darwin_deployment_target,
  )
  releases.package_core(release_ctx, "fixture")
  let archive_path = fp"{artifact_dir}/core-fixture.tar.xz"
  let entries = archive.tar_list(archive_path)?.collect()
  let installed = entries |> where .path == "core/system-report"
  assert installed.len() == 1
  assert installed[0].mode.bit_and(0o777) == 0o755
  let extracted = fp"{artifact_dir}/extracted"
  archive.tar_extract(archive_path, extracted)
  assert fp"{extracted}/core/system-report".read_bytes()? == fp"{repository}/core/system-report.xsh".read_bytes()?
  assert fp"{extracted}/core/lib/system_report.xsh".exists()?
}

test test_release_checksum_sidecars_keep_a_relative_artifact_name { |ctx|
  let root = test.temp_dir(ctx, name: "release-checksum")?
  let artifact = fp"{root}/dist/xsh-release-x86_64-linux-musl"
  artifact.parent().mkdir()
  artifact.write("release artifact")
  let checksum = releases.checksum_line(artifact, root)?
  assert """  dist/xsh-release-x86_64-linux-musl
""" in checksum, checksum
}

test test_release_validation_requires_exactly_the_nine_expected_products { |ctx|
  let root = test.temp_dir(ctx, name: "release-validation")?
  let artifact_dir = fp"{root}/dist"
  artifact_dir.mkdir()
  let release_ctx = fixtures.linux_context(root)?
  let tag = "release-test"

  for triple in [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "aarch64-apple-darwin",
  ] {
    let suffix = target_policy.release_suffix(triple)?

    for product in target_policy.products {
      let artifact = fp"{artifact_dir}/{product}-{tag}-{suffix}"
      artifact.write("release artifact", mode: 0o755)
      fp"{artifact}.sha256".write(releases.checksum_line(artifact, root)?)
    }
  }

  releases.validate_artifacts(release_ctx, tag)
  fp"{artifact_dir}/unexpected-file".write("not a release artifact")

  match releases.validate_artifacts(release_ctx, tag) {
    Ok(_) => test.fail("unexpected artifact passed validation")
    Err(error) => assert "StageError.Failed" in error.message, error.message
  }
}

test test_unsupported_target_remains_a_structured_error {
  match target_policy.resolve("riscv64-unknown-linux-musl") {
    Ok(_) => test.fail("unsupported target resolved")
    Err(error) => assert error.message == "TargetError.Unsupported"
  }
}

test test_context_paths_and_missing_tools_have_named_failures {
  assert lifecycle.repo_path(/repo, "target/custom") == "/repo/target/custom"
  assert lifecycle.repo_path(/repo, "/tmp/custom") == "/tmp/custom"

  match stages.require_tool("xsh-selfhost-test-tool-that-does-not-exist") {
    Ok(_) => test.fail("missing tool unexpectedly resolved")
    Err(error) => assert error.message == "StageError.MissingTool"
  }
}

test test_failed_stage_reports_its_stage_and_target {
  match stages.execute(
    stages.command(
      "selfhost-stage",
      "selfhost-target",
      "false",
      ["false"],
      p".",
      {},
    ),
  ) {
    Ok(_) => test.fail("failing command unexpectedly succeeded")
    Err(error) => assert error.message == "StageError.Failed"
  }
}

test test_subprocess_wrong_directory_has_a_named_failure { |ctx|
  let repository = fs.cwd()?
  let module_path = fp"{repository}/dev".display()
  let wrong_directory = test.run_script(
    ctx,
    """
use context

cd p"/" {
  match context.require_root() {
    Ok(_) => exit 1
    Err(error) => print \${error.message}
  }
}?
""",
    [],
    {XSH_MODULE_PATH: module_path},
  )?
  assert wrong_directory.success, f"""{wrong_directory.stdout}
{wrong_directory.stderr}"""
  assert "ContextError.WrongDirectory" in wrong_directory.stdout, wrong_directory.stdout
}

test test_context_target_and_docker_platform_overrides { |ctx|
  let repository = fs.cwd()?
  let module_path = fp"{repository}/dev".display()
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
  assert context_default.success, context_default.stderr
  assert context_override.success, context_override.stderr
  let uname = system.uname()?
  let expected_default = target_policy.host_default_triple(
    target_policy.host_os_tag(uname.sysname)?,
    target_policy.host_arch_tag(uname.machine)?,
  )?
  assert context_default.stdout.trim() == expected_default
  assert context_override.stdout.trim() == "aarch64-unknown-linux-musl"

  let platform = test.expect(
    ctx,
    """
use context
use docker
use targets as target_policy

let ctx: context.Context = {
  root: p"/repo",
  target_dir: p"/repo/target",
  coverage_dir: p"/repo/target/cov",
  artifact_dir: p"/repo/dist",
  host_os: target_policy.Linux,
  host_arch: target_policy.X86_64,
  target: target_policy.resolve("x86_64-unknown-linux-musl")?,
  profile: "dist",
  darwin_deployment_target: "26.0",
}
proc main() [env, error] -> Result[Unit] {
  print \${docker.platform(ctx)?}
}

main()?
""",
    status: 0,
    args: [],
    env: {XSH_MODULE_PATH: module_path, DOCKER_PLATFORM: "linux/override"},
  )?
  assert platform.stdout.trim() == "linux/override"
}

test test_dev_main_target_override_reaches_context { |ctx|
  let root = fs.cwd()?
  let output = test.temp_path(ctx, name: "dev-target.stdout")
  let stderr = test.temp_path(ctx, name: "dev-target.stderr")
  let status = run.status ${ctx.xsh_bin} fp"{root}/dev/main.xsh" -- system-report-check --target \
    x86_64-unknown-linux-musl > $output 2> $stderr
  let exited_successfully = status.exited_with(0)
  let diagnostic = stderr.read_text()?
  assert exited_successfully, diagnostic
  assert "system-report coverage manifest" in output.read_text()?
}

test test_rustybench_override_stays_a_direct_argv_prefix { |ctx|
  let repository = fs.cwd()?
  let module_path = fp"{repository}/dev".display()

  let rustybench = test.run_script(
    ctx,
    """
use bench
use context
use targets as target_policy

let ctx: context.Context = {
  root: p"/repo",
  target_dir: p"/repo/target",
  coverage_dir: p"/repo/target/cov",
  artifact_dir: p"/repo/dist",
  host_os: target_policy.Linux,
  host_arch: target_policy.X86_64,
  target: target_policy.resolve("x86_64-unknown-linux-musl")?,
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
  assert rustybench.success, f"""{rustybench.stdout}
{rustybench.stderr}"""
  assert rustybench.stdout.trim() == "cargo|run|--quiet|--manifest-path|/tmp/rustybench/Cargo.toml|--", rustybench.stdout
}


test test_gate_docker_argv_mounts_worktree_git_before_the_image {
  let ctx = fixtures.linux_context(/repo, "verification")?
  let argv = docker_workflows.internal_argv(ctx, "docker", "linux/amd64", "gate-lane", false, 1000, 1000, "", ["src/sema/check/records.rs"], git_common_dir: p"/common/.git")?
  assert "/common/.git:/common/.git:ro" in argv
  assert argv[0] == "docker"
  assert argv[-1] == "src/sema/check/records.rs"
  assert argv[-2] == "gate-lane"
}

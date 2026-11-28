##! Typed lifecycle fixtures shared by native development workflow tests.
use context
use targets as target_policy

## Constructs the default Linux lifecycle context.
export pure linux_context(root: Path, profile: Str = "dist") -> Result[context.Context, Error] {
  Ok({
    root: root,
    target_dir: fp"{root}/target",
    coverage_dir: fp"{root}/target/cov",
    artifact_dir: fp"{root}/dist",
    host_os: target_policy.Linux,
    host_arch: target_policy.X86_64,
    target: target_policy.resolve("x86_64-unknown-linux-musl")?,
    profile: profile,
    darwin_deployment_target: "26.0",
  })
}

## Constructs the Darwin lifecycle context.
export pure darwin_context(root: Path, profile: Str = "dist") -> Result[context.Context, Error] {
  Ok({
    root: root,
    target_dir: fp"{root}/target",
    coverage_dir: fp"{root}/target/cov",
    artifact_dir: fp"{root}/dist",
    host_os: target_policy.Darwin,
    host_arch: target_policy.Aarch64,
    target: target_policy.resolve("aarch64-apple-darwin")?,
    profile: profile,
    darwin_deployment_target: "26.0",
  })
}

## Constructs the Linux ARM lifecycle context used by Docker argv tests.
export pure linux_aarch64_context(root: Path, profile: Str = "dist") -> Result[context.Context, Error] {
  Ok({
    root: root,
    target_dir: fp"{root}/target",
    coverage_dir: fp"{root}/target/cov",
    artifact_dir: fp"{root}/dist",
    host_os: target_policy.Linux,
    host_arch: target_policy.Aarch64,
    target: target_policy.resolve("aarch64-unknown-linux-musl")?,
    profile: profile,
    darwin_deployment_target: "26.0",
  })
}

## Writes a `cc` into `tools` that reports a musl target. A musl build step
## then needs no host cross compiler, so a test that puts `tools` first on
## PATH behaves the same on every host.
export proc musl_host_compiler(tools: Path) [fs, error] -> Result[Unit, Error] {
  fp"{tools}/cc".write("#!/bin/sh\nif [ \"$1\" = -dumpmachine ]; then echo x86_64-linux-musl; exit 0; fi\nexit 1\n", mode: 0o755)
}

##! Typed lifecycle fixtures shared by native development workflow tests.
use context
use targets as target_policy

## Constructs the default Linux lifecycle context.
export pure linux_context(root: Path, profile: Str = "dist") -> Result[context.Context] {
  return Ok({
    root: root,
    target_dir: fp"${root}/target",
    coverage_dir: fp"${root}/target/cov",
    artifact_dir: fp"${root}/dist",
    host_os: target_policy.Linux,
    host_arch: target_policy.X86_64,
    target: target_policy.resolve("x86_64-unknown-linux-musl")?,
    profile: profile,
    darwin_deployment_target: "26.0",
  })
}

## Constructs the Darwin lifecycle context.
export pure darwin_context(root: Path, profile: Str = "dist") -> Result[context.Context] {
  return Ok({
    root: root,
    target_dir: fp"${root}/target",
    coverage_dir: fp"${root}/target/cov",
    artifact_dir: fp"${root}/dist",
    host_os: target_policy.Darwin,
    host_arch: target_policy.Aarch64,
    target: target_policy.resolve("aarch64-apple-darwin")?,
    profile: profile,
    darwin_deployment_target: "26.0",
  })
}

## Constructs the Linux ARM lifecycle context used by Docker argv tests.
export pure linux_aarch64_context(root: Path, profile: Str = "dist") -> Result[context.Context] {
  return Ok({
    root: root,
    target_dir: fp"${root}/target",
    coverage_dir: fp"${root}/target/cov",
    artifact_dir: fp"${root}/dist",
    host_os: target_policy.Linux,
    host_arch: target_policy.Aarch64,
    target: target_policy.resolve("aarch64-unknown-linux-musl")?,
    profile: profile,
    darwin_deployment_target: "26.0",
  })
}

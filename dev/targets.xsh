##! Closed target policy for XSH's development lifecycle.
## Keep architecture, toolchain, Docker, and verification properties here.
## All properties used by build, container, and verification policy.
export type Target = {
  triple: Str,
  id: TargetId,
  os: Str,
  arch: Str,
  docker_platform: Str,
  executable_format: Str,
  elf_machine: Str,
  cpu_rustflags: List[Str],
  cpu_cflags: List[Str],
  static_musl: Bool,
}

## Closed internal host operating-system policy.
export type HostOs = Linux | Darwin

## Closed internal host architecture policy.
export type HostArch = X86_64 | Aarch64

## Closed target matrix identity used by workflow policy.
type TargetId = X86_64LinuxMusl | Aarch64LinuxMusl | Aarch64AppleDarwin

## A target or host selector outside the supported matrix.
export error TargetError = Unsupported(target: Str)

## The ordinary local distribution target.
export let default_triple = "x86_64-unknown-linux-musl"

## Product binaries that belong in every distribution.
export let products = ["xsh", "xsht", "xshi"]

## Cargo features used by release-like distribution builds.
export let distribution_features = "xsh/net xsh/tools xsht/native-tests"

## Classifies the host operating system reported by `system.uname`.
export pure host_os(sysname: Str) -> Result[Str] {
  match sysname {
    "Linux" => return "linux"
    "Darwin" => return "darwin"
    _ => return Err(TargetError.Unsupported(target: f"host OS ${sysname}"))
  }
}

## Decodes the host operating-system boundary into closed policy.
export pure host_os_tag(sysname: Str) -> Result[HostOs] {
  match sysname {
    "Linux" => return Linux
    "Darwin" => return Darwin
    _ => return Err(TargetError.Unsupported(target: f"host OS ${sysname}"))
  }
}

## Classifies the host architecture reported by `system.uname`.
export pure host_arch(machine: Str) -> Result[Str] {
  match machine {
    "x86_64" => return "x86_64"
    "amd64" => return "x86_64"
    "aarch64" => return "aarch64"
    "arm64" => return "aarch64"
    _ => return Err(TargetError.Unsupported(target: f"host architecture ${machine}"))
  }
}

## Decodes the host architecture boundary into closed policy.
export pure host_arch_tag(machine: Str) -> Result[HostArch] {
  match machine {
    "x86_64" => return X86_64
    "amd64" => return X86_64
    "aarch64" => return Aarch64
    "arm64" => return Aarch64
    _ => return Err(TargetError.Unsupported(target: f"host architecture ${machine}"))
  }
}

## Decodes a supported target triple into its closed identity.
export pure target_id(triple: Str) -> Result[TargetId] {
  match triple {
    "x86_64-unknown-linux-musl" => return X86_64LinuxMusl
    "aarch64-unknown-linux-musl" => return Aarch64LinuxMusl
    "aarch64-apple-darwin" => return Aarch64AppleDarwin
    _ => return Err(TargetError.Unsupported(target: triple))
  }
}

## Resolves one supported Rust target triple into its complete policy record.
export pure resolve(triple: Str) -> Result[Target] {
  match triple {
    "x86_64-unknown-linux-musl" => return {
      triple: triple,
      id: X86_64LinuxMusl,
      os: "linux",
      arch: "x86_64",
      docker_platform: "linux/amd64",
      executable_format: "ELF",
      elf_machine: "Advanced Micro Devices X86-64",
      cpu_rustflags: [
        "-C",
        "target-cpu=x86-64-v3",
      ],
      cpu_cflags: [
        "-march=x86-64-v3",
      ],
      static_musl: true,
    }
    "aarch64-unknown-linux-musl" => return {
      triple: triple,
      id: Aarch64LinuxMusl,
      os: "linux",
      arch: "aarch64",
      docker_platform: "linux/arm64",
      executable_format: "ELF",
      elf_machine: "AArch64",
      cpu_rustflags: [
        "-C",
        "target-cpu=neoverse-n2",
        "-C",
        "target-feature=-sve,-sve2",
      ],
      cpu_cflags: [
        "-mcpu=neoverse-n2+nosve+nosve2",
      ],
      static_musl: true,
    }
    "aarch64-apple-darwin" => return {
      triple: triple,
      id: Aarch64AppleDarwin,
      os: "darwin",
      arch: "aarch64",
      docker_platform: "linux/arm64",
      executable_format: "Mach-O",
      elf_machine: "",
      cpu_rustflags: [
        "-C",
        "target-cpu=apple-m1",
      ],
      cpu_cflags: [
        "-mcpu=apple-m1",
      ],
      static_musl: false,
    }
    _ => return Err(TargetError.Unsupported(target: triple))
  }
}

## Reports whether a target can execute directly on a classified host.
export pure can_execute_natively(target: Target, os: HostOs, arch: HostArch) -> Bool {
  match target.id {
    X86_64LinuxMusl => return os == Linux and arch == X86_64
    Aarch64LinuxMusl => return os == Linux and arch == Aarch64
    Aarch64AppleDarwin => return os == Darwin and arch == Aarch64
  }
}

## Returns Cargo's directory name for a selected profile.
export pure profile_directory(profile: Str) -> Str {
  if profile == "release" {
    return "release"
  }

  return profile
}

## Combines inherited and XSH-owned Linux distribution compiler flags.
export pure target_rustflags(target: Target, inherited: Str) -> Str {
  let common = ["-Zlocation-detail=none", "-Zunstable-options", "-Cpanic=immediate-abort"]
  var flags: List[Str] = []

  if inherited.trim() != "" {
    flags = flags.push(inherited.trim())
  }

  flags = flags.extend(common).extend(target.cpu_rustflags)

  if target.static_musl {
    flags = flags.extend(
      [
        "-C",
        "target-feature=+crt-static",
        "-C",
        "link-arg=--defsym=__isoc23_sscanf=sscanf",
        "-C",
        "link-arg=--defsym=__isoc23_strtol=strtol",
      ],
    )
  }

  return flags.join(" ")
}

## Combines inherited and XSH-owned Darwin distribution compiler flags.
export pure darwin_rustflags(target: Target, inherited: Str) -> Str {
  var flags: List[Str] = []

  if inherited.trim() != "" {
    flags = flags.push(inherited.trim())
  }

  return flags.extend(["-C", "linker=rust-lld", "-C", "linker-flavor=ld64.lld", "-C", "link-arg=--icf=safe"])
    .extend(["-Zlocation-detail=none", "-Zunstable-options", "-Cpanic=immediate-abort"])
    .extend(target.cpu_rustflags)
    .join(" ")
}

## Produces target-specific Cargo environment overrides without mutating the parent environment.
export pure distribution_env(
  triple: Str,
  inherited_rustflags: Str,
  inherited_cflags: Str,
  deployment_target: Str,
) -> Result[Record] {
  let target = resolve(triple)?

  if target.os == "linux" and target.arch == "x86_64" {
    return {
      CFLAGS_x86_64_unknown_linux_musl: ([
        inherited_cflags.trim(),
        target.cpu_cflags.join(" "),
      ]
        |> where . != ""
        |> collect()).join(" "),
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: target_rustflags(
        target,
        inherited_rustflags,
      ),
    }
  }

  if target.os == "linux" and target.arch == "aarch64" {
    return {
      CFLAGS_aarch64_unknown_linux_musl: ([
        inherited_cflags.trim(),
        target.cpu_cflags.join(" "),
      ]
        |> where . != ""
        |> collect()).join(" "),
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: target_rustflags(
        target,
        inherited_rustflags,
      ),
    }
  }

  if target.os == "darwin" and target.arch == "aarch64" {
    return {
      MACOSX_DEPLOYMENT_TARGET: deployment_target,
      CFLAGS_aarch64_apple_darwin: ([
        inherited_cflags.trim(),
        target.cpu_cflags.join(" "),
      ]
        |> where . != ""
        |> collect()).join(" "),
      CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS: darwin_rustflags(target, inherited_rustflags),
    }
  }

  return Err(TargetError.Unsupported(target: triple))
}

## Reports whether selected tagged target and host policies can execute directly.
export pure native_execution(target: Target, host_os: HostOs, host_arch: HostArch) -> Bool {
  return can_execute_natively(target, host_os, host_arch)
}

## Maps a target triple to the stable release artifact suffix.
export pure release_suffix(triple: Str) -> Result[Str] {
  match triple {
    "x86_64-unknown-linux-musl" => return "x86_64-linux-musl"
    "aarch64-unknown-linux-musl" => return "aarch64-linux-musl"
    "aarch64-apple-darwin" => return "aarch64-apple-darwin"
    _ => return Err(TargetError.Unsupported(target: triple))
  }
}

## Returns the target-specific C compiler environment variable name.
export pure cflags_variable(triple: Str) -> Result[Str] {
  match triple {
    "x86_64-unknown-linux-musl" => return "CFLAGS_x86_64_unknown_linux_musl"
    "aarch64-unknown-linux-musl" => return "CFLAGS_aarch64_unknown_linux_musl"
    "aarch64-apple-darwin" => return "CFLAGS_aarch64_apple_darwin"
    _ => return Err(TargetError.Unsupported(target: triple))
  }
}

## Produces the static-link flags needed by Linux container test builds.
export pure docker_test_env(triple: Str) -> Result[Record] {
  let flags = [
    "-C target-feature=+crt-static",
    "-C link-arg=--defsym=__isoc23_sscanf=sscanf",
    "-C link-arg=--defsym=__isoc23_strtol=strtol",
  ].join(" ")

  match triple {
    "x86_64-unknown-linux-musl" => return {CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: flags}
    "aarch64-unknown-linux-musl" => return {CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: flags}
    _ => return Err(TargetError.Unsupported(target: triple))
  }
}

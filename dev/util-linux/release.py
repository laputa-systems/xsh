from __future__ import annotations

from collections.abc import Callable
from contextvars import ContextVar
from dataclasses import dataclass
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
from typing import Protocol, TypeVar

import resources
from acceptance import BuildContext, SourceIdentity


BUILD_TIMEOUT_SECONDS = 1800
TARGET = resources.TARGET
PRODUCT_ALLOCATION = "1cpu/1GiB"
BUILD_CPUS = 2
BUILD_MEMORY_GIB = 8
BUILD_JOBS = 2
BUILD_COMMAND = (
    "/root/.cargo/bin/cargo",
    "build",
    "--locked",
    "--offline",
    "--release",
    "--target",
    TARGET,
    "-p",
    "xsh",
    "--bins",
    "-p",
    "xsht",
    "--bin",
    "xsht",
    "-p",
    "xsh",
    "--bin",
    "xsh-test-helper",
)
RUSTFLAGS = (
    "-C target-feature=+crt-static "
    "-C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf "
    "-C link-arg=-Wl,--defsym=__isoc23_strtol=strtol"
)
PRODUCTS = ("xsh", "xsht", "xsh-test-helper")
JEMALLOC = Path("/usr/lib/libjemalloc.so.2")
_RESULT_PREFIX = b"@@xsh-util-linux-release:"


class ReleaseRuntime(Protocol):
    ROOT: Path
    IMAGE: str

    def source_identity(self) -> SourceIdentity: ...

    def container(
        self,
        script: str,
        *,
        mounts: list[str],
        capabilities: list[str],
        timeout: int,
        cpus: int = 1,
        memory_gib: int = 1,
    ) -> bytes: ...


@dataclass(frozen=True)
class _ReleaseProducts:
    source: SourceIdentity
    directory: Path
    context: BuildContext
    signature: resources.ProductSignature


@dataclass(frozen=True)
class _ActiveScope:
    runtime: ReleaseRuntime
    products: _ReleaseProducts


_ACTIVE: ContextVar[_ActiveScope | None] = ContextVar("util_linux_release_products", default=None)
_T = TypeVar("_T")


def _assert_current(runtime: ReleaseRuntime, source: SourceIdentity, operation: str) -> None:
    if (
        not source.clean
        or re.fullmatch(r"[0-9a-f]{40}", source.commit) is None
        or re.fullmatch(r"[0-9a-f]{40}", source.tree) is None
    ):
        raise ValueError(f"{operation} requires a clean source identity")
    if runtime.source_identity() != source:
        raise ValueError(f"source changed before or during {operation}")


def _git_common_dir(root: Path) -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git common directory lookup failed: {result.stderr.strip()}")
    value = result.stdout.strip()
    if not value:
        raise ValueError("git returned an empty common directory")
    common = Path(value)
    if not common.is_absolute() or not common.is_dir():
        raise ValueError("git common directory is not an existing absolute directory")
    return common


def _build_script(source: SourceIdentity) -> str:
    environment = {
        "AR": "/usr/local/bin/llvm-ar",
        "CARGO_HOME": "/root/.cargo",
        "CARGO_BUILD_JOBS": str(BUILD_JOBS),
        "CARGO_NET_OFFLINE": "true",
        "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16",
        "CARGO_PROFILE_RELEASE_LTO": "thin",
        "CARGO_PROFILE_RELEASE_OPT_LEVEL": "3",
        "CARGO_TARGET_DIR": "/cargo-target",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER": "/usr/local/bin/xsh-mold-linker",
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS": RUSTFLAGS,
        "CC": "/usr/bin/clang-23",
        "LD_PRELOAD": "/usr/lib/libjemalloc.so.2",
        "PATH": "/usr/local/bin:/root/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        "RUSTC_WRAPPER": "/usr/local/bin/kache",
        "RUSTUP_HOME": "/root/.rustup",
        "TARGET": TARGET,
        "TMPDIR": "/t",
        "XSH_LAPUTA_CORPUS": str(resources.LAPUTA_CORPUS),
    }
    exports = "export " + " ".join(f"{name}={shlex.quote(value)}" for name, value in environment.items())
    command = shlex.join(BUILD_COMMAND)
    checks = " && ".join(f"test -f /cargo-target/{TARGET}/release/{name} && test -x /cargo-target/{TARGET}/release/{name}" for name in PRODUCTS)
    marker = (_RESULT_PREFIX + source.commit.encode() + b":" + source.tree.encode() + b":complete").decode()
    return (
        "cd /work; "
        + exports
        + "; "
        + command
        + "; "
        + checks
        + "; printf '%s\\n' "
        + shlex.quote(marker)
    )


def _completion(output: bytes, source: SourceIdentity) -> None:
    marker = _RESULT_PREFIX + source.commit.encode() + b":" + source.tree.encode() + b":complete"
    lines = output.splitlines()
    if not lines or lines[-1] != marker or lines.count(marker) != 1:
        raise ValueError("release container omitted its exact source-bound completion marker")


def _product_context(runtime: ReleaseRuntime, source: SourceIdentity) -> BuildContext:
    return BuildContext(
        source=source.commit + ":" + source.tree,
        profile="release",
        target=TARGET,
        image=runtime.IMAGE,
        allocator="mimalloc",
        allocation=PRODUCT_ALLOCATION,
    )


def _build_products(runtime: ReleaseRuntime, source: SourceIdentity, target: Path) -> _ReleaseProducts:
    _assert_current(runtime, source, "release build")
    common_dir = _git_common_dir(runtime.ROOT)
    mounts = resources._cargo_mounts(runtime, target)
    if not JEMALLOC.is_file():
        raise RuntimeError("release build requires the pinned jemalloc preload")
    mounts.append(f"type=bind,source={JEMALLOC},target=/usr/lib/libjemalloc.so.2,readonly")
    git_mount = f"type=bind,source={common_dir},target={common_dir},readonly"
    if (runtime.ROOT / ".git").resolve(strict=True) != common_dir.resolve(strict=True):
        mounts.append(git_mount)
    _assert_current(runtime, source, "release build")
    output = runtime.container(
        _build_script(source),
        mounts=mounts,
        capabilities=[],
        timeout=BUILD_TIMEOUT_SECONDS,
        cpus=BUILD_CPUS,
        memory_gib=BUILD_MEMORY_GIB,
    )
    _completion(output, source)
    _assert_current(runtime, source, "release product qualification")
    directory = target / TARGET / "release"
    signature = resources._product_signature(directory)
    _assert_current(runtime, source, "release product qualification")
    return _ReleaseProducts(
        source=source,
        directory=directory,
        context=_product_context(runtime, source),
        signature=signature,
    )


def _validate_active(products: _ReleaseProducts, runtime: ReleaseRuntime) -> None:
    _assert_current(runtime, products.source, "release product use")
    if resources._product_signature(products.directory) != products.signature:
        raise ValueError("release products changed during their scoped use")


def active_products(runtime: ReleaseRuntime) -> tuple[Path, BuildContext] | None:
    scope = _ACTIVE.get()
    if scope is None:
        return None
    if scope.runtime is not runtime:
        raise ValueError("active release products belong to another runtime")
    _validate_active(scope.products, runtime)
    return scope.products.directory, scope.products.context


def _remove_target(runtime: ReleaseRuntime, target: Path) -> None:
    script = (
        "python3 -c "
        + shlex.quote(
            "from pathlib import Path; import shutil; "
            "root = Path('/cargo-target'); "
            "[(shutil.rmtree(path) if path.is_dir() and not path.is_symlink() else path.unlink()) "
            "for path in root.iterdir()]"
        )
    )
    runtime.container(
        script,
        mounts=[f"type=bind,source={target},target=/cargo-target,rw"],
        capabilities=[],
        timeout=30,
    )
    if any(target.iterdir()):
        raise RuntimeError("pinned-container cleanup left files in the release target")


def _run_active(
    runtime: ReleaseRuntime,
    products: _ReleaseProducts,
    operation: Callable[[], _T],
) -> _T:
    failure: BaseException | None = None
    try:
        result = operation()
    except BaseException as error:
        failure = error
        result = None
    try:
        _validate_active(products, runtime)
    except BaseException as error:
        failure = error if failure is None else BaseExceptionGroup(
            "release operation and product validation both failed", [failure, error]
        )
    if failure is not None:
        raise failure
    return result


def with_release_products(
    runtime: ReleaseRuntime,
    source: SourceIdentity,
    operation: Callable[[], _T],
) -> _T:
    active = _ACTIVE.get()
    if active is not None:
        if active.runtime is not runtime:
            raise ValueError("active release products belong to another runtime")
        if active.products.source != source:
            raise ValueError("nested release product scope changed source identity")
        _validate_active(active.products, runtime)
        return _run_active(runtime, active.products, operation)
    _assert_current(runtime, source, "release product scope")
    scratch = runtime.ROOT / ".work" / "tmp"
    work = runtime.ROOT / ".work"
    if work.is_symlink() or not work.is_dir() or scratch.is_symlink() or not scratch.is_dir():
        raise ValueError("release products require the ignored repository scratch directory")
    build_root = Path(tempfile.mkdtemp(prefix="util-linux-release-", dir=scratch))
    container_attempted = False
    failure: BaseException | None = None
    try:
        container_attempted = True
        products = _build_products(runtime, source, build_root)
        token = _ACTIVE.set(_ActiveScope(runtime, products))
        try:
            result = _run_active(runtime, products, operation)
        except BaseException as error:
            failure = error
            raise
        finally:
            _ACTIVE.reset(token)
        return result
    except BaseException as error:
        if failure is None:
            failure = error
        raise
    finally:
        cleanup_errors = []
        if container_attempted:
            try:
                _remove_target(runtime, build_root)
            except BaseException as error:
                cleanup_errors.append(error)
        try:
            shutil.rmtree(build_root)
        except BaseException as error:
            cleanup_errors.append(error)
        if cleanup_errors:
            cleanup_error = cleanup_errors[0] if len(cleanup_errors) == 1 else BaseExceptionGroup(
                "release target cleanup had multiple failures", cleanup_errors
            )
            if failure is None:
                raise cleanup_error
            raise BaseExceptionGroup(
                "release operation and temporary product cleanup both failed",
                [failure, cleanup_error],
            )

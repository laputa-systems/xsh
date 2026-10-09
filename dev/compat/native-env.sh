#!/usr/bin/env bash
# Source this file for campaign commands on a native Linux x86_64 host.

case "$(uname -s):$(uname -m)" in
  Linux:x86_64) ;;
  *)
    printf 'The compatibility campaign setup supports native Linux x86_64 only.\n' >&2
    return 2 2>/dev/null || exit 2
    ;;
esac

XSH_REPO_ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
XSH_COMMON_DIR=$(cd "$XSH_REPO_ROOT" && git rev-parse --path-format=absolute --git-common-dir)
XSH_WORKSPACE_ROOT=$(dirname "$(dirname "$XSH_COMMON_DIR")")
XSH_TOOLS_ROOT=${XSH_TOOLS_ROOT:-$XSH_WORKSPACE_ROOT/.tools}
export XSH_TOOLS_ROOT
export CARGO_HOME=${CARGO_HOME:-$XSH_TOOLS_ROOT/cargo}
export RUSTUP_HOME=${RUSTUP_HOME:-$XSH_TOOLS_ROOT/rustup}
export PATH="$XSH_REPO_ROOT/target/release:$CARGO_HOME/bin:$XSH_TOOLS_ROOT/gnu-env/usr/bin:$PATH:$XSH_TOOLS_ROOT/gnu-env/bin:$XSH_TOOLS_ROOT/bin"

if ! command -v mold >/dev/null 2>&1 || ! mold --version | grep -q '3\.0\.0'; then
  printf 'Install mold 3.0.0 under %s/bin before building.\n' "$CARGO_HOME" >&2
  return 2 2>/dev/null || exit 2
fi

case " ${RUSTFLAGS-} " in
  *" -C link-arg=-fuse-ld=mold "*) ;;
  *) RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }-C link-arg=-fuse-ld=mold" ;;
esac
export RUSTFLAGS

export UUTILS_ROOT=${UUTILS_ROOT:-$(dirname "$XSH_REPO_ROOT")/ref/uutils-coreutils}
export GNU_ROOT=${GNU_ROOT:-$(dirname "$XSH_REPO_ROOT")/ref/gnu-coreutils}
export XSH_LAPUTA_CORPUS=${XSH_LAPUTA_CORPUS:-$(dirname "$XSH_REPO_ROOT")/laputa}
export UUTESTS_THREADS=${UUTESTS_THREADS:-3}
export GNU_JOBS=${GNU_JOBS:-3}

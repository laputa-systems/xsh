#!/bin/sh
# Reproduce crates/oxish-client from the pinned ssh-build revision.
#   sh scripts/vendor-ssh-client.sh verify   diff the vendored files against the pinned revision
#   sh scripts/vendor-ssh-client.sh update   replace the vendored files with the pinned revision
# The revision is the Revision line of crates/oxish-client/vendor/ORIGIN.md. SSH_BUILD_DIR names an
# existing ssh-build checkout; otherwise the revision is fetched into .work/sources/ssh-build.
# Cargo.toml files, vendor/ORIGIN.md, the licenses and fixture/ are local and never compared.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
root_dir=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
crate=$root_dir/crates/oxish-client
work_dir=$root_dir/.work
upstream_url=https://github.com/laputa-systems/ssh-build
excluded_modules="cli copy progress sftp subsystem terminal"
excluded_tests="sftp sftp_peer interactive keygen"

fail() { echo "vendor-ssh-client: $*" >&2; exit 1; }

rev=$(sed -n 's/^Revision: `\([0-9a-f]\{40\}\)`$/\1/p' "$crate/vendor/ORIGIN.md")
[ "${#rev}" -eq 40 ] || fail "cannot read the Revision line of crates/oxish-client/vendor/ORIGIN.md"

upstream=${SSH_BUILD_DIR:-$work_dir/sources/ssh-build}
if [ ! -d "$upstream/.git" ]; then
    [ -z "${SSH_BUILD_DIR:-}" ] || fail "$SSH_BUILD_DIR is not a git checkout"
    mkdir -p "$work_dir/sources"
    git clone --quiet --no-checkout "$upstream_url" "$upstream"
fi
git -C "$upstream" cat-file -e "$rev^{commit}" 2>/dev/null || git -C "$upstream" fetch --quiet origin "$rev"
git -C "$upstream" cat-file -e "$rev^{commit}" 2>/dev/null || fail "$rev is not in the ssh-build checkout"

check=$work_dir/ssh-client-vendor
rm -rf "$check"
mkdir -p "$check/out/vendor"
git -C "$upstream" archive "$rev" client/src client/tests vendor/graviola vendor/oxish/oxish-proto vendor/oxish/oxish-graviola |
    tar -x -C "$check"

mv "$check/client/src" "$check/out/src"
mv "$check/client/tests" "$check/out/tests"
mv "$check/vendor/graviola" "$check/out/vendor/graviola"
mv "$check/vendor/oxish/oxish-proto" "$check/out/vendor/oxish-proto"
mv "$check/vendor/oxish/oxish-graviola" "$check/out/vendor/oxish-graviola"

rm -rf "$check/out/src/bin"
for module in $excluded_modules; do
    rm -f "$check/out/src/$module.rs"
    grep -v "^pub mod $module;\$" "$check/out/src/lib.rs" > "$check/lib.rs"
    mv "$check/lib.rs" "$check/out/src/lib.rs"
done
for test in $excluded_tests; do
    rm -f "$check/out/tests/$test.rs"
done

dirs="src tests vendor/graviola vendor/oxish-proto vendor/oxish-graviola"
case ${1:-} in
    verify)
        status=0
        for dir in $dirs; do
            diff -r -x Cargo.toml -x target "$check/out/$dir" "$crate/$dir" > "$check/diff" || {
                head -n 20 "$check/diff" >&2
                echo "vendor-ssh-client: $dir differs from ssh-build $rev" >&2
                status=1
            }
        done
        [ "$status" -eq 0 ] || exit 1
        echo "crates/oxish-client == ssh-build $rev"
        ;;
    update)
        for dir in $dirs; do
            find "$crate/$dir" -type f ! -name Cargo.toml ! -path '*/target/*' -delete
            (cd "$check/out/$dir" && find . -type f ! -name Cargo.toml -exec sh -c 'for f; do mkdir -p "$0/$(dirname "$f")"; cp "$f" "$0/$f"; done' "$crate/$dir" {} +)
            find "$crate/$dir" -depth -type d -empty -exec rmdir {} \;
        done
        echo "updated crates/oxish-client from ssh-build $rev"
        ;;
    *) fail "usage: vendor-ssh-client.sh verify|update" ;;
esac

#!/usr/bin/env bash
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
scratch_root="$repo_root/.work/tmp"
target_root="$repo_root/.work/target-final-canonical"
image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c
target=x86_64-unknown-linux-musl
rustflags='-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol'

fail() {
    printf '%s\n' "$*" >&2
    exit 2
}

if [[ $# -ne 2 ]]; then
    fail "usage: $0 FINAL_SOURCE_HEAD FINAL_SOURCE_TREE_OID"
fi
source_head=$1
source_tree=$2
[[ "$source_head" =~ ^[0-9a-f]{40}$ ]] || fail "source HEAD must be a full lowercase Git object ID"
[[ "$source_tree" =~ ^[0-9a-f]{40}$ ]] || fail "source tree must be a full lowercase Git object ID"
[[ "$(git rev-parse HEAD)" == "$source_head" ]] || fail "source HEAD is not the designated freeze"
[[ "$(git rev-parse 'HEAD^{tree}')" == "$source_tree" ]] || fail "source tree is not the designated freeze"
[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || fail "source checkout must be clean"
[[ -d "$target_root" && ! -L "$target_root" ]] || fail "canonical target directory is missing or unsafe: $target_root"
[[ -x "$target_root/$target/verification/xsh" ]] || fail "verification xsh is missing from the canonical target directory"
git check-ignore --quiet --no-index .work || fail ".work must be ignored through .git/info/exclude"

for command in docker jaq sha256sum realpath; do
    command -v "$command" >/dev/null 2>&1 || fail "required host tool is missing: $command"
done
for path in /home/josh/.cargo/bin/kache /usr/local/bin/mold /usr/lib/libjemalloc.so.2 dev/musl-linker.sh; do
    [[ -f "$path" && -r "$path" ]] || fail "required build input is missing: $path"
done

run_dir=$(mktemp -d "$scratch_root/acceptance-products-449.XXXXXXXX")
export TMPDIR="$scratch_root"
git rev-parse HEAD 'HEAD^{tree}' >"$run_dir/source-before.txt"
cp "$0" "$run_dir/build.sh"
common_git=$(git rev-parse --path-format=absolute --git-common-dir)
container_name="xsh-priv-harness-${run_dir##*.}"

docker_base=(
    docker run --rm --init --platform linux/amd64
    --cpus=12 --memory=24g --memory-swap=24g --network=none
    --name "$container_name"
    --volume "$repo_root:/work:ro"
    --volume "$target_root:/work/target:rw"
    --volume "$scratch_root:/scratch:rw"
    --volume xsh-cargo-registry:/root/.cargo/registry
    --volume xsh-consolidation-kache:/root/.cache/kache
    --volume "$common_git:$common_git:ro"
    --volume /home/josh/.cargo/bin/kache:/usr/local/bin/kache:ro
    --volume /usr/local/bin/mold:/usr/local/bin/mold:ro
    --volume "$repo_root/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro"
    --volume /usr/lib/libjemalloc.so.2:/usr/lib/libjemalloc.so.2:ro
    --workdir /work
    --env "TARGET=$target"
    --env "CARGO_TARGET_DIR=/work/target"
    --env "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker"
    --env "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=$rustflags"
    --env CARGO_BUILD_WARNINGS=deny
    --env CARGO_BUILD_JOBS=12
    --env CARGO_NET_OFFLINE=true
    --env CARGO_HOME=/root/.cargo
    --env RUSTUP_HOME=/root/.rustup
    --env XDG_CACHE_HOME=/root/.cache
    --env RUSTC_WRAPPER=/usr/local/bin/kache
    --env TMPDIR=/scratch
)

run_cargo_target() {
    local target_name=$1
    shift
    local json_path="$run_dir/$target_name.jsonl"
    local stderr_path="$run_dir/$target_name.stderr"
    if "${docker_base[@]}" "$image" cargo test --locked --offline \
        --message-format=json-render-diagnostics \
        --profile verification --target "$target" -p xsh "$@" --no-run \
        >"$json_path" 2>"$stderr_path"; then
        :
    else
        local exit_status=$?
        cat "$stderr_path" >&2
        fail "Cargo no-run build failed for $target_name with status $exit_status; output: $run_dir"
    fi
}

artifact_path() {
    local target_name=$1
    local expected_name=$2
    local json_path="$run_dir/$target_name.jsonl"
    local paths
    local filter
    filter="select(.reason == \"compiler-artifact\" and .target.name == \"$target_name\" and .executable != null) | .executable"
    paths=$(jaq -r "$filter" "$json_path")
    [[ -n "$paths" ]] || fail "Cargo JSON did not report an executable for $target_name; output: $json_path"
    local -a reported_paths=()
    mapfile -t reported_paths <<< "$paths"
    [[ ${#reported_paths[@]} -eq 1 ]] || fail "Cargo JSON reported ${#reported_paths[@]} executables for $target_name"
    local reported=${reported_paths[0]}
    [[ "$reported" =~ ^/work/target/x86_64-unknown-linux-musl/verification/build/xsh/[[:xdigit:]]{16}/out/${expected_name}-[[:xdigit:]]{16}$ ]] ||
        fail "Cargo executable path is outside the observed verification build layout: $reported"
    local stage_rel=${reported#/work/}
    local host_path="$target_root/${reported#/work/target/}"
    [[ -f "$host_path" && -x "$host_path" && ! -L "$host_path" ]] || fail "Cargo-reported host executable is missing or unsafe: $host_path"
    local digest
    digest=$(sha256sum -- "$host_path" | cut -d ' ' -f 1)
    printf '%s\t%s\t%s\t%s\t%s\n' "$target_name" "$reported" "$stage_rel" "$host_path" "$digest"
}

run_cargo_target core_compat_extra --features linux-priv-tests --test core_compat_extra
run_cargo_target core_compat_extra_privileged --features linux-priv-tests --test core_compat_extra_privileged
"${docker_base[@]}" "$image" cargo build --locked --offline \
    --message-format=json-render-diagnostics --profile verification --target "$target" \
    -p xsh -p xsht -p xshi --bins >"$run_dir/default-products.jsonl" 2>"$run_dir/default-products.stderr"

xsh_path="$target_root/$target/verification/xsh"
xsh_sha256=$(sha256sum -- "$xsh_path" | cut -d ' ' -f 1)
{
    printf 'xsh\t/work/target/%s/verification/xsh\ttarget/%s/verification/xsh\t%s\t%s\n' "$target" "$target" "$xsh_path" "$xsh_sha256"
    artifact_path core_compat_extra core_compat_extra
    artifact_path core_compat_extra_privileged core_compat_extra_privileged
} > "$run_dir/stage-identities.tsv"

[[ "$(git rev-parse HEAD)" == "$source_head" ]] || fail "source HEAD changed during Cargo builds"
[[ "$(git rev-parse 'HEAD^{tree}')" == "$source_tree" ]] || fail "source tree changed during Cargo builds"
[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || fail "source checkout became dirty during Cargo builds"

for product in xsh xsht xshi; do
    reported=$(jaq -r --arg product "$product" 'select(.reason == "compiler-artifact" and .target.name == $product and .executable != null) | .executable' "$run_dir/default-products.jsonl")
    [[ "$reported" == "/work/target/$target/verification/$product" ]] || fail "unexpected Cargo binary artifact: $reported"
    sha256sum "$target_root/$target/verification/$product" >> "$run_dir/default-products.sha256"
done
git rev-parse HEAD 'HEAD^{tree}' >"$run_dir/source-after.txt"
cmp "$run_dir/source-before.txt" "$run_dir/source-after.txt"
cat "$run_dir/stage-identities.tsv"
cat "$run_dir/default-products.sha256"
printf 'cargo_json=%s\n' "$run_dir"

#!/bin/bash
set -euo pipefail
: "${SOURCE_ROOT:?set SOURCE_ROOT to the frozen source worktree}"
: "${PRODUCT_DIR:?set PRODUCT_DIR to the qualified product directory}"
: "${SCRATCH_ROOT:?set SCRATCH_ROOT to worktree .work/tmp}"
: "${IMAGE:?set IMAGE to the pinned test image digest}"
: "${SOURCE_COMMIT:?set SOURCE_COMMIT to the frozen source commit}"
: "${SOURCE_TREE:?set SOURCE_TREE to the frozen source tree}"
: "${COMMON_ROOT:?set COMMON_ROOT to the common repository root}"
: "${PRODUCT_XSH_SHA256:?set PRODUCT_XSH_SHA256 from the canonical qualification}"
: "${PRODUCT_XSHT_SHA256:?set PRODUCT_XSHT_SHA256 from the canonical qualification}"
source_commit=$(git -C "$SOURCE_ROOT" rev-parse HEAD)
source_tree=$(git -C "$SOURCE_ROOT" rev-parse 'HEAD^{tree}')
source_status=$(git -C "$SOURCE_ROOT" status --porcelain)
if [[ "$source_commit" != "$SOURCE_COMMIT" || "$source_tree" != "$SOURCE_TREE" || -n "$source_status" ]]; then
  printf 'source guard rejected: commit=%s status=%q\n' "$source_commit" "$source_status" >&2
  exit 2
fi
case "$PRODUCT_DIR" in
  /home/josh/d/laputa-systems/xsh/.work/target-final-canonical|/home/josh/d/laputa-systems/xsh/.work/target-final-canonical/*)
    printf 'product guard rejected the old main-root target cache\n' >&2
    exit 2
    ;;
esac
test -d "$PRODUCT_DIR"
test -x "$PRODUCT_DIR/xsh"
test -x "$PRODUCT_DIR/xsht"
git_common_dir=$(git -C "$SOURCE_ROOT" rev-parse --path-format=absolute --git-common-dir)
if [[ "$git_common_dir" != "$COMMON_ROOT/.git" ]]; then
  printf 'Git common directory mismatch: expected=%s/.git actual=%s\n' "$COMMON_ROOT" "$git_common_dir" >&2
  exit 2
fi
sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}
case "${1:-}" in
  standard)
    context=standard
    selector_file=linux-residual-selectors-default.txt
    expected=35
    capabilities=(--cap-drop=SYS_ADMIN)
    ;;
  sysadmin)
    context=sysadmin
    selector_file=linux-residual-selector-sysadmin.txt
    expected=1
    capabilities=(--cap-add=SYS_ADMIN)
    ;;
  *)
    printf 'context must be standard or sysadmin\n' >&2
    exit 2
    ;;
esac
log="$SCRATCH_ROOT/linux-residual-$context.log"
results="/w/.work/tmp/linux-residual-$context-results.log"
current="/w/.work/tmp/linux-residual-$context-current.log"
product_xsh_before=$(sha256_file "$PRODUCT_DIR/xsh")
product_xsht_before=$(sha256_file "$PRODUCT_DIR/xsht")
printf 'source_commit=%s\nsource_tree=%s\nsource_status=clean\n' "$source_commit" "$source_tree" > "$log"
printf 'git_common_dir=%s\n' "$git_common_dir" >> "$log"
printf 'product_xsh_sha256_expected=%s\nproduct_xsh_sha256_before=%s\n' "$PRODUCT_XSH_SHA256" "$product_xsh_before" >> "$log"
printf 'product_xsht_sha256_expected=%s\nproduct_xsht_sha256_before=%s\n' "$PRODUCT_XSHT_SHA256" "$product_xsht_before" >> "$log"
if [[ "$product_xsh_before" != "$PRODUCT_XSH_SHA256" || "$product_xsht_before" != "$PRODUCT_XSHT_SHA256" ]]; then
  printf 'qualified product hash mismatch before execution\n' >> "$log"
  cat "$log"
  exit 2
fi
cidfile="$SCRATCH_ROOT/linux-residual-$context-$$.cid"
container_name="linux-residual-$context-$$"
docker_pid=
docker_status=unset
signal_status=0
cleanup_status=0
cleanup_done=0
guards_recorded=0
container_id=unavailable

inspect_owned_container() {
  local identifier=$1 output status
  output=$(docker container inspect --format '{{.Id}}' "$identifier" 2>&1)
  status=$?
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$output"
    return 0
  fi
  if [[ "$output" == *'No such object'* || "$output" == *'No such container'* ]]; then
    return 1
  fi
  printf 'container inspect failed for %s: %s\n' "$identifier" "$output" >> "$log"
  return 2
}

cleanup_owned_container() {
  local identifier inspection_status
  if [[ "$cleanup_done" -eq 1 ]]; then
    return "$cleanup_status"
  fi
  cleanup_done=1
  if [[ -s "$cidfile" ]]; then
    identifier=$(tr -d '\r\n' < "$cidfile")
  else
    identifier=$(inspect_owned_container "$container_name")
    inspection_status=$?
    if [[ "$inspection_status" -eq 1 ]]; then
      container_id=not-created
      printf 'container_name=%s\ncontainer_terminal=absent\n' "$container_name" >> "$log"
      return 0
    fi
    if [[ "$inspection_status" -ne 0 ]]; then
      cleanup_status=1
      printf 'container_name=%s\ncontainer_terminal=unverified\n' "$container_name" >> "$log"
      return 1
    fi
  fi
  if [[ ! "$identifier" =~ ^[0-9a-f]{64}$ ]]; then
    container_id=invalid
    cleanup_status=1
    printf 'container_id_invalid=%q\ncontainer_terminal=unverified\n' "$identifier" >> "$log"
    return 1
  fi
  container_id=$identifier
  printf 'container_id=%s\n' "$container_id" >> "$log"
  inspect_owned_container "$container_id" >/dev/null
  inspection_status=$?
  if [[ "$inspection_status" -eq 0 ]]; then
    docker rm --force --time 0 "$container_id" >> "$log" 2>&1
  elif [[ "$inspection_status" -ne 1 ]]; then
    cleanup_status=1
  fi
  inspect_owned_container "$container_id" >/dev/null
  inspection_status=$?
  if [[ "$inspection_status" -eq 0 ]]; then
    printf 'container_cleanup_retry=1\n' >> "$log"
    docker rm --force --time 0 "$container_id" >> "$log" 2>&1
    inspect_owned_container "$container_id" >/dev/null
    inspection_status=$?
  fi
  if [[ "$inspection_status" -eq 1 ]]; then
    printf 'container_terminal=absent\n' >> "$log"
  else
    printf 'container_terminal=present_or_unverified\n' >> "$log"
    cleanup_status=1
  fi
  return "$cleanup_status"
}

record_post_guards() {
  if [[ "$guards_recorded" -eq 1 ]]; then
    return 0
  fi
  guards_recorded=1
  local source_commit_after source_tree_after source_status_after product_xsh_after product_xsht_after
  source_commit_after=$(git -C "$SOURCE_ROOT" rev-parse HEAD 2>&1)
  source_tree_after=$(git -C "$SOURCE_ROOT" rev-parse 'HEAD^{tree}' 2>&1)
  source_status_after=$(git -C "$SOURCE_ROOT" status --porcelain 2>&1)
  product_xsh_after=$(sha256_file "$PRODUCT_DIR/xsh")
  product_xsht_after=$(sha256_file "$PRODUCT_DIR/xsht")
  printf 'source_commit_after=%s\nsource_tree_after=%s\nsource_status_after=%q\n' "$source_commit_after" "$source_tree_after" "$source_status_after" >> "$log"
  printf 'product_xsh_sha256_after=%s\nproduct_xsht_sha256_after=%s\n' "$product_xsh_after" "$product_xsht_after" >> "$log"
  if [[ "$source_commit_after" != "$source_commit" || "$source_tree_after" != "$source_tree" || "$source_commit_after" != "$SOURCE_COMMIT" || "$source_tree_after" != "$SOURCE_TREE" || -n "$source_status_after" || "$product_xsh_after" != "$product_xsh_before" || "$product_xsht_after" != "$product_xsht_before" || "$product_xsh_after" != "$PRODUCT_XSH_SHA256" || "$product_xsht_after" != "$PRODUCT_XSHT_SHA256" ]]; then
    printf 'source or product guard changed during execution\n' >> "$log"
    return 1
  fi
  return 0
}

finish() {
  local final_status=$?
  trap - EXIT INT TERM HUP
  if [[ -n "$docker_pid" ]]; then
    kill -TERM "$docker_pid" 2>/dev/null || true
    if wait "$docker_pid"; then
      docker_status=0
    else
      docker_status=$?
    fi
    docker_pid=
  fi
  if [[ "$docker_status" == unset ]]; then
    docker_status=$final_status
  fi
  cleanup_owned_container || true
  printf 'docker_exit=%s cleanup_exit=%s signal_exit=%s\n' "$docker_status" "$cleanup_status" "$signal_status" >> "$log"
  record_post_guards || final_status=1
  if [[ "$cleanup_status" -ne 0 ]]; then
    final_status=1
  elif [[ "$final_status" -eq 0 && "$docker_status" -ne 0 ]]; then
    final_status=$docker_status
  elif [[ "$final_status" -eq 0 && "$signal_status" -ne 0 ]]; then
    final_status=$signal_status
  fi
  cat "$log"
  exit "$final_status"
}

on_signal() {
  local signal=$1
  case "$signal" in
    INT) signal_status=130 ;;
    TERM) signal_status=143 ;;
    HUP) signal_status=129 ;;
  esac
  exit "$signal_status"
}

trap finish EXIT
trap 'on_signal INT' INT
trap 'on_signal TERM' TERM
trap 'on_signal HUP' HUP

argv=(docker run --rm --init --cidfile "$cidfile" --name "$container_name" --platform linux/amd64 --network=none --cpus=4 --memory=8g --memory-swap=8g --user=0:0 --group-add=123 --group-add=456 --cap-add=CHOWN --cap-add=SETUID --cap-add=SETGID --cap-add=SETPCAP --cap-add=SYS_CHROOT --cap-add=NET_RAW "${capabilities[@]}" --volume "$SOURCE_ROOT:/repo:ro" --volume "$PRODUCT_DIR:/target:ro" --volume "$SCRATCH_ROOT:/w/.work/tmp:rw" --volume "$git_common_dir:$git_common_dir:ro" --workdir=/repo --env XSH_BIN=/target/xsh --env XSHT_BIN=/target/xsht --env "SELECTORS=/w/.work/tmp/$selector_file" --env "EXPECTED_COUNT=$expected" --env "RESULTS=$results" --env "CURRENT=$current" --env "COMMON_ROOT=$COMMON_ROOT" --env "SOURCE_COMMIT=$SOURCE_COMMIT" --env "SOURCE_TREE=$SOURCE_TREE" --env "PRODUCT_XSH_SHA256=$PRODUCT_XSH_SHA256" --env "PRODUCT_XSHT_SHA256=$PRODUCT_XSHT_SHA256" --env TMPDIR=/w/.work/tmp/t --env HOST_UID=1000 --env HOST_GID=1000 --env HOME=/root --env LC_ALL=C --env GIT_OPTIONAL_LOCKS=0 --env GIT_CONFIG_COUNT=1 --env GIT_CONFIG_KEY_0=safe.directory --env GIT_CONFIG_VALUE_0=/repo --entrypoint=/bin/sh "$IMAGE" /w/.work/tmp/linux-residual-run-native.sh)
{
  printf 'context=%s\n' "$context"
  printf 'docker_argv='
  printf ' %q' "${argv[@]}"
  printf '\n'
} >> "$log"
printf 'container_name=%s\ncidfile=%s\n' "$container_name" "$cidfile" >> "$log"
"${argv[@]}" >> "$log" 2>&1 &
docker_pid=$!
set +e
wait "$docker_pid"
docker_status=$?
docker_pid=
set -e
cleanup_owned_container || true
printf 'docker_exit=%s cleanup_exit=%s signal_exit=%s\n' "$docker_status" "$cleanup_status" "$signal_status" >> "$log"
record_post_guards
guards_status=$?
if [[ "$cleanup_status" -ne 0 ]]; then
  exit 1
fi
if [[ "$guards_status" -ne 0 ]]; then
  exit 1
fi
exit "$docker_status"

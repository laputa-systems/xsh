#!/usr/bin/env bash
set -Eeuo pipefail
readonly expected_head=5277f62b7b8b81f9805cb0d8e33172db5c1d8c9f
readonly expected_tree=c5ffd8a209d8e6b59c7012c571d64ed5436f5290
readonly expected_xsh=ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154
readonly expected_xsht=c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c

snapshot() {
  git -C /work rev-parse HEAD 'HEAD^{tree}'
  git -C /work status --porcelain=v1 --untracked-files=all
  [[ $(git -C /work rev-parse HEAD) == "$expected_head" ]] || return 1
  [[ $(git -C /work rev-parse 'HEAD^{tree}') == "$expected_tree" ]] || return 1
  [[ -z $(git -C /work status --porcelain=v1 --untracked-files=all) ]] || return 1
  printf '%s  /target/xsh\n%s  /target/xsht\n' "$expected_xsh" "$expected_xsht" | sha256sum --check
}

finish() {
  local primary=$?
  trap - EXIT
  if ! snapshot > /scratch/inside-after.txt 2> /scratch/inside-after.stderr; then
    printf 'inside source or product guard failed\n' >&2
    if (( primary == 0 )); then primary=75; fi
  fi
  exit "$primary"
}
trap finish EXIT
[[ $(id -u) == 1000 && $(id -g) == 1000 ]]
snapshot > /scratch/inside-before.txt
readonly cases=(
  test_typing_host_result_anchor_and_nested_loops
  test_typing_host_constraints_follow_chains_and_body_order
  test_typing_host_replacement_result_keeps_receiver_ambiguity
  test_typing_host_byte_receiver_keeps_ambiguity_after_bytes_caller
  test_typing_host_inference_preserves_argument_and_effect_errors
  test_typing_host_untemplated_receiver_domains_remain_candidates
)
: > /scratch/summary.tsv
for name in "${cases[@]}"; do
  selector=tests/xsh/typing-capability-witnesses.xsh::$name
  if /target/xsht test --exact "$selector" -j 1 > "/scratch/$name.stdout" 2> "/scratch/$name.stderr"; then status=0; else status=$?; fi
  printf '%s\t%s\n' "$status" "$selector" >> /scratch/summary.tsv
  [[ "$status" -eq 0 ]]
  awk -v expected="$selector" '
    $0 == "running 1 tests" { selected++ }
    $0 == "test result: ok. 1 passed; 0 failed; 0 skipped" { summary++ }
    index($0, expected " ... ok ") == 1 { passed++ }
    END { exit !(selected == 1 && summary == 1 && passed == 1) }
  ' "/scratch/$name.stdout"
done
cat /scratch/summary.tsv

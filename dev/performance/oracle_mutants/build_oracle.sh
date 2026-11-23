build_oracle() {
  local plan=$1 log=$2 count
  count=$(jaq -er '.nodes | length | select(. > 0)' "$plan") || return 1
  ! rg -q '^repo build (start|done|FAILED) ' "$log" || return 1
  [[ $(rg -c "^repo build [0-9a-f]+ $count artifacts$" "$log") == 1 ]] || return 1
  printf 'reused=%s builds=0\n' "$count"
}

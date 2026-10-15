set -u
overall=0
index=0
while IFS= read -r filter; do
  [ -n "$filter" ] || continue
  index=$((index + 1))
  name=${filter##*::}
  prefix=$(printf '/scratch/tests/%02d-%s' "$index" "$name")
  printf '%s\n' "$ARTIFACT $filter --exact --nocapture" > "$prefix.argv.txt"
  if "$ARTIFACT" "$filter" --exact --nocapture > "$prefix.stdout" 2> "$prefix.stderr"; then
    status=0
  else
    status=$?
    overall=1
  fi
  printf '%s\n' "$status" > "$prefix.status"
  printf '%s\t%s\n' "$status" "$filter"
done < /scratch/selected-filters.txt
printf 'overall_status=%s\n' "$overall" > /scratch/tests/summary.status
exit "$overall"

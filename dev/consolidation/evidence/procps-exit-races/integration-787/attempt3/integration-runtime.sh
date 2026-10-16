set +e
unset LD_PRELOAD
mkdir -p /scratch/runtime-tmp /scratch/home
printf '%s\n' 'runtime_image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c' 'architecture=linux/amd64' 'cpu_cap=2' 'memory_cap=4g' 'swap=none' 'network=none' 'runtime_preload=unset' 'source=/work at 787ba37a43e808c6b63b85b17cc912fe36352b22' 'products=/artifact verification' > /scratch/runtime-environment.txt
python3 /scratch/run-capped-probe.py
probe_runner_status=$?
printf '%s\n' "$probe_runner_status" > /scratch/churn-runner.status
if [ "$probe_runner_status" -ne 0 ] || [ ! -f /scratch/iostat-probe.result ]; then
  printf 'churn probe setup failed runner_status=%s\n' "$probe_runner_status" > /scratch/first-failure.txt
  exit 31
fi
if [ "$(cat /scratch/churn-container.timed-out)" = true ]; then
  printf 'bounded churn probe inconclusive: deadline interrupted an in-flight iostat call; see captured child status\n' > /scratch/first-failure.txt
  exit 37
fi
probe_status=$(sed -n 's/^status=//p' /scratch/iostat-probe.result | head -n 1)
if grep -Fq 'reason=expected header absent' /scratch/iostat-probe.result; then
  printf 'iostat returned success without the expected device header\n' > /scratch/first-failure.txt
  exit 35
fi
if [ "$probe_status" = 3 ]; then
  printf 'iostat failed during bounded churn probe\n' > /scratch/first-failure.txt
  exit 32
fi
if [ "$probe_status" != 0 ]; then
  printf 'unexpected bounded churn probe status: %s\n' "$probe_status" > /scratch/first-failure.txt
  exit 34
fi
if ! grep -Fq 'reason=500 completed iterations' /scratch/iostat-probe.result; then
  printf 'bounded churn probe did not complete all 500 iterations\n' > /scratch/first-failure.txt
  exit 38
fi
run_case() {
  name=$1
  directory=$2
  shift 2
  mkdir -p "/scratch/runtime-tmp/$name"
  printf 'cwd=%s\nargv=' "$directory" > "/scratch/$name.argv.txt"
  printf '%q ' "$@" >> "/scratch/$name.argv.txt"
  printf '\n' >> "/scratch/$name.argv.txt"
  (
    cd "$directory" || exit 125
    export TMPDIR="/scratch/runtime-tmp/$name"
    export HOME=/scratch/home
    unset LD_PRELOAD
    "$@"
  ) > "/scratch/$name.stdout" 2> "/scratch/$name.stderr"
  case_status=$?
  printf '%s\n' "$case_status" > "/scratch/$name.status"
  if [ "$case_status" -ne 0 ]; then
    printf '%s exited %s\n' "$name" "$case_status" > /scratch/first-failure.txt
    return "$case_status"
  fi
  return 0
}
run_case written-procps /work /artifact/xsht test core/tests/test-procps.xsh
case_status=$?
if [ "$case_status" -ne 0 ]; then exit 40; fi
if ! grep -Fq 'test result: ok. 39 passed; 0 failed; 0 skipped' /scratch/written-procps.stdout; then
  printf 'written procps test file did not report the expected 39 passes\n' > /scratch/first-failure.txt
  exit 44
fi
rm -rf /scratch/generated-workspace
mkdir -p /scratch/generated-workspace/core/lib /scratch/generated-workspace/core/tests
cp /work/xsht-config.ini /scratch/generated-workspace/xsht-config.ini
emit_desugared() {
  relative=$1
  destination="/scratch/generated-workspace/$relative"
  mkdir -p "$(dirname "$destination")"
  printf '/artifact/xsht desugar /work/%s\n' "$relative" > "$destination.argv.txt"
  /artifact/xsht desugar "/work/$relative" > "$destination" 2> "$destination.desugar.stderr"
  desugar_status=$?
  printf '%s\n' "$desugar_status" > "$destination.desugar.status"
  if [ "$desugar_status" -ne 0 ]; then
    printf 'desugar failed for %s with status %s\n' "$relative" "$desugar_status" > /scratch/first-failure.txt
    return "$desugar_status"
  fi
  return 0
}
for relative in \
  core/free.xsh \
  core/fuser.xsh \
  core/iostat.xsh \
  core/killall.xsh \
  core/lib/gnu.xsh \
  core/lib/procps.xsh \
  core/lsof.xsh \
  core/pidof.xsh \
  core/pidstat.xsh \
  core/pgrep.xsh \
  core/pkill.xsh \
  core/ps.xsh \
  core/tests/test-procps.xsh \
  core/top.xsh \
  core/vmstat.xsh \
  core/watch.xsh; do
  emit_desugared "$relative"
  desugar_status=$?
  if [ "$desugar_status" -ne 0 ]; then exit 41; fi
done
sha256sum \
  /scratch/generated-workspace/xsht-config.ini \
  /scratch/generated-workspace/core/free.xsh \
  /scratch/generated-workspace/core/fuser.xsh \
  /scratch/generated-workspace/core/iostat.xsh \
  /scratch/generated-workspace/core/killall.xsh \
  /scratch/generated-workspace/core/lib/gnu.xsh \
  /scratch/generated-workspace/core/lib/procps.xsh \
  /scratch/generated-workspace/core/lsof.xsh \
  /scratch/generated-workspace/core/pidof.xsh \
  /scratch/generated-workspace/core/pidstat.xsh \
  /scratch/generated-workspace/core/pgrep.xsh \
  /scratch/generated-workspace/core/pkill.xsh \
  /scratch/generated-workspace/core/ps.xsh \
  /scratch/generated-workspace/core/tests/test-procps.xsh \
  /scratch/generated-workspace/core/top.xsh \
  /scratch/generated-workspace/core/vmstat.xsh \
  /scratch/generated-workspace/core/watch.xsh > /scratch/desugared-source.sha256
run_case desugared-procps /scratch/generated-workspace /artifact/xsht test core/tests/test-procps.xsh
case_status=$?
if [ "$case_status" -ne 0 ]; then exit 42; fi
if ! grep -Fq 'test result: ok. 39 passed; 0 failed; 0 skipped' /scratch/desugared-procps.stdout; then
  printf 'desugared procps test file did not report the expected 39 passes\n' > /scratch/first-failure.txt
  exit 45
fi
run_case linux-sampling-malformed /work /artifact/xsht test --exact tests/xsh/stdlib/linux_sampling.xsh::test_linux_sample_rejects_missing_and_malformed_required_counters
case_status=$?
if [ "$case_status" -ne 0 ]; then exit 43; fi
if ! grep -Fq 'test_linux_sample_rejects_missing_and_malformed_required_counters' /scratch/linux-sampling-malformed.stdout || ! grep -Fq '1 passed; 0 failed' /scratch/linux-sampling-malformed.stdout; then
  printf 'malformed-stat native check did not report its expected pass\n' > /scratch/first-failure.txt
  exit 46
fi
printf '%s\n' 'focused runtime completed successfully' > /scratch/runtime-complete.txt
exit 0

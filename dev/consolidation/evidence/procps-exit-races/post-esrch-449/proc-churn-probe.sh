#!/bin/sh
set -u
workers=
probe_pid=
iterations=0
cleanup() {
  trap - EXIT TERM INT HUP
  if [ -n "$probe_pid" ]; then
    kill -TERM "$probe_pid" 2>/dev/null || :
    wait "$probe_pid" 2>/dev/null || :
    probe_pid=
  fi
  for pid in $workers; do kill -TERM "$pid" 2>/dev/null || :; done
  for pid in $workers; do wait "$pid" 2>/dev/null || :; done
  workers=
}
on_timeout() {
  trap - TERM INT HUP
  if [ -n "$probe_pid" ]; then
    kill -TERM "$probe_pid" 2>/dev/null || :
    if wait "$probe_pid"; then probe_status=0; else probe_status=$?; fi
    probe_pid=
  else
    probe_status=none
  fi
  printf 'iterations=%s\nstatus=timeout\nchild_status=%s\nworkers=8\nmax_churn_children=8\nargv=/artifact/xsh /work/core/iostat.xsh -- -d -m 0.01 1\nreason=22-second deadline\n' "$iterations" "$probe_status" > /scratch/iostat-probe.result
  exit 124
}
trap 'cleanup; exit 143' INT HUP
trap cleanup EXIT
trap on_timeout TERM

worker=0
while [ "$worker" -lt 8 ]; do
  (
    child=
    stop_worker() {
      trap - TERM INT HUP
      if [ -n "$child" ]; then
        kill -TERM "$child" 2>/dev/null || :
        wait "$child" 2>/dev/null || :
      fi
      exit 0
    }
    trap stop_worker TERM INT HUP
    while :; do
      /bin/true &
      child=$!
      wait "$child" 2>/dev/null || :
      child=
    done
  ) >/dev/null 2>&1 &
  workers="$workers $!"
  worker=$((worker + 1))
done

while [ "$iterations" -lt 1000 ]; do
  iterations=$((iterations + 1))
  /artifact/xsh /work/core/iostat.xsh -- -d -m 0.01 1 > /scratch/iostat-probe.stdout 2> /scratch/iostat-probe.stderr &
  probe_pid=$!
  if wait "$probe_pid"; then
    probe_status=0
  else
    probe_status=$?
  fi
  probe_pid=
  if [ "$probe_status" -ne 0 ]; then
    printf 'iterations=%s\nstatus=%s\nworkers=8\nmax_churn_children=8\nargv=/artifact/xsh /work/core/iostat.xsh -- -d -m 0.01 1\n' "$iterations" "$probe_status" > /scratch/iostat-probe.result
    exit 0
  fi
  if ! grep -Fq 'Device tps MB_read/s MB_wrtn/s MB_read MB_wrtn' /scratch/iostat-probe.stdout; then
    printf 'iterations=%s\nstatus=%s\nworkers=8\nmax_churn_children=8\nargv=/artifact/xsh /work/core/iostat.xsh -- -d -m 0.01 1\nreason=expected header absent\n' "$iterations" "$probe_status" > /scratch/iostat-probe.result
    exit 0
  fi
done
printf 'iterations=%s\nstatus=0\nworkers=8\nmax_churn_children=8\nargv=/artifact/xsh /work/core/iostat.xsh -- -d -m 0.01 1\nreason=sample limit reached\n' "$iterations" > /scratch/iostat-probe.result

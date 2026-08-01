#!/bin/sh

set -u

docker_bin=${DOCKER:-docker}
platform=${PLATFORM:-linux/arm64}
gym_dir=${GYM_DIR:?GYM_DIR is required}
task_image=${TASK_IMAGE:?TASK_IMAGE is required}
session_volume=${SESSION_VOLUME:?SESSION_VOLUME is required}
work_dir=${WORK_DIR:?WORK_DIR is required}
output_dir=${OUTPUT_DIR:?OUTPUT_DIR is required}
pi_binary=${PI_BINARY:-}
pi_command=${PI_COMMAND:-pi}
pi_provider=${PI_PROVIDER:-openrouter}
pi_model=${PI_MODEL:-deepseek/deepseek-v4-flash-0731}
pi_thinking=${PI_THINKING:-high}
pi_telemetry=${PI_TELEMETRY:-0}
pi_offline=${PI_OFFLINE:-1}
pi_auth_file=${PI_AUTH_FILE:?PI_AUTH_FILE is required}
pi_agent_dir=${PI_AGENT_DIR:-/run/pi-agent}

mkdir -p "$work_dir" "$output_dir"
cp "$gym_dir/agents.md" "$gym_dir/handbook.md" "$gym_dir/task-ecount.md" "$work_dir/"
rm -f \
  "$work_dir/ecount.xsh" \
  "$output_dir/session.jsonl" \
  "$output_dir/session.html" \
  "$output_dir/run.json" \
  "$output_dir/ecount.xsh" \
  "$output_dir/candidate.stdout" \
  "$output_dir/oracle.stdout"

test -f "$pi_auth_file" || {
  echo "Pi auth file does not exist: $pi_auth_file" >&2
  exit 2
}

if test -n "$pi_binary"; then
  test -f "$pi_binary" || {
    echo "PI_BINARY does not exist: $pi_binary" >&2
    exit 2
  }
fi

# A fixed name makes the session location discoverable during a run. It is
# removed only after the evaluator has copied the session and manifest out.
"$docker_bin" volume rm "$session_volume" >/dev/null 2>&1 || true
"$docker_bin" volume create "$session_volume" >/dev/null

image_id=$("$docker_bin" image inspect --format '{{.Id}}' "$task_image")

set --
set -- "$@" --rm --platform "$platform"
set -- "$@" --read-only --tmpfs '/tmp:rw,noexec,nosuid,nodev'
set -- "$@" --tmpfs "$pi_agent_dir:rw,noexec,nosuid,nodev"
set -- "$@" --cap-drop=ALL --security-opt=no-new-privileges
if test -n "$pi_binary"; then
  set -- "$@" --mount "type=bind,src=$pi_binary,dst=/usr/local/bin/pi,readonly"
fi
set -- "$@" --mount "type=bind,src=$pi_auth_file,dst=/run/pi-auth.json,readonly"
set -- "$@" --mount "type=bind,src=$work_dir,dst=/work"
set -- "$@" --mount "type=volume,src=$session_volume,dst=/session"
set -- "$@" --workdir /work
set -- "$@" --env "PI_COMMAND=$pi_command"
set -- "$@" --env "PI_PROVIDER=$pi_provider"
set -- "$@" --env "PI_MODEL=$pi_model"
set -- "$@" --env "PI_THINKING=$pi_thinking"
set -- "$@" --env "PI_TELEMETRY=$pi_telemetry"
set -- "$@" --env "PI_OFFLINE=$pi_offline"
set -- "$@" --env "PI_CODING_AGENT_DIR=$pi_agent_dir"
set -- "$@" --mount "type=bind,src=$work_dir/agents.md,dst=/work/agents.md,readonly"
set -- "$@" --mount "type=bind,src=$work_dir/handbook.md,dst=/work/handbook.md,readonly"
set -- "$@" --mount "type=bind,src=$work_dir/task-ecount.md,dst=/work/task-ecount.md,readonly"

agent_status=0
# The single-quoted script is evaluated inside the container, not by this host
# shell; its variables intentionally expand there.
# shellcheck disable=SC2016
"$docker_bin" run "$@" "$task_image" /bin/sh -eu -c '
  mkdir -p "$PI_CODING_AGENT_DIR"
  cp /run/pi-auth.json "$PI_CODING_AGENT_DIR/auth.json"
  chmod 600 "$PI_CODING_AGENT_DIR/auth.json"
  rm -f /session/task-ecount-session.jsonl /session/task-ecount-session.html
  if ! command -v "$PI_COMMAND" >/dev/null 2>&1; then
    echo "pi is not in the task image; set PI_BINARY to a Linux arm64 release" >&2
    exit 127
  fi
  agent_status=0
  "$PI_COMMAND" \
    --provider "$PI_PROVIDER" \
    --model "$PI_MODEL" \
    --thinking "$PI_THINKING" \
    --approve \
    --system-prompt /work/agents.md \
    --no-extensions \
    --no-skills \
    --no-prompt-templates \
    --no-themes \
    --no-context-files \
    --tools read,write,edit,bash \
    --session /session/task-ecount-session.jsonl \
    --print \
    @/work/task-ecount.md \
    "Complete task-ecount.md in /work. Run the required checks and leave the requested artifact there." &
  pi_pid=$!
  # Stream the session file as pi persists it: session entries are appended
  # and flushed synchronously, so tail -f shows each entry as it is written.
  # The file appears only once the first assistant message lands, so wait for
  # it (bounded) before starting the tail.
  retries=0
  while test ! -f /session/task-ecount-session.jsonl && test "$retries" -lt 3000; do
    retries=$((retries + 1))
    sleep 0.1
  done
  if test -f /session/task-ecount-session.jsonl; then
    tail -f /session/task-ecount-session.jsonl &
    tail_pid=$!
  fi
  if wait "$pi_pid"; then
    agent_status=0
  else
    agent_status=$?
  fi
  # Give the tail a moment to flush its last buffered lines before stopping it.
  sleep 0.2
  if test -n "${tail_pid:-}"; then
    kill "$tail_pid" 2>/dev/null || true
    wait "$tail_pid" 2>/dev/null || true
  fi
  if test -f /session/task-ecount-session.jsonl; then
    "$PI_COMMAND" --export /session/task-ecount-session.jsonl /session/task-ecount-session.html || true
  fi
  exit "$agent_status"
'
agent_status=$?

set --
set -- "$@" --rm --platform "$platform"
set -- "$@" --read-only --tmpfs '/tmp:rw,noexec,nosuid,nodev'
set -- "$@" --cap-drop=ALL --security-opt=no-new-privileges
set -- "$@" --mount "type=bind,src=$work_dir,dst=/work,readonly"
set -- "$@" --mount "type=volume,src=$session_volume,dst=/session"
set -- "$@" --mount "type=bind,src=$output_dir,dst=/export"
set -- "$@" --workdir /work
set -- "$@" --env "GYM_IMAGE_ID=$image_id"
set -- "$@" --env "GYM_PLATFORM=$platform"
set -- "$@" --env "PI_PROVIDER=$pi_provider"
set -- "$@" --env "PI_MODEL=$pi_model"
set -- "$@" --env "PI_THINKING=$pi_thinking"
set -- "$@" --env "PI_TELEMETRY=$pi_telemetry"
set -- "$@" --env "PI_OFFLINE=$pi_offline"

eval_status=0
# The single-quoted script is evaluated inside the container, not by this host
# shell; its variables intentionally expand there.
# shellcheck disable=SC2016
"$docker_bin" run "$@" "$task_image" /bin/sh -eu -c '
  copy_results() {
    cp /session/task-ecount-session.jsonl /export/session.jsonl 2>/dev/null || true
    cp /session/task-ecount-session.html /export/session.html 2>/dev/null || true
    cp /session/run.json /export/run.json 2>/dev/null || true
    cp /session/candidate.stdout /export/candidate.stdout 2>/dev/null || true
    cp /session/oracle.stdout /export/oracle.stdout 2>/dev/null || true
    cp /work/ecount.xsh /export/ecount.xsh 2>/dev/null || true
  }
  trap copy_results 0

  eval_status=0
  if test -f /work/ecount.xsh; then
    xsh /work/ecount.xsh /usr/share > /session/candidate.stdout || eval_status=$?
    if test "$eval_status" -eq 0; then
      fd --color=never -tf . /usr/share | awk -F. "NF > 1 {print tolower(\$NF)}" | sort | uniq -c | sort -n > /session/oracle.stdout || eval_status=$?
    fi
    if test "$eval_status" -eq 0 && cmp -s /session/candidate.stdout /session/oracle.stdout; then
      echo "task-ecount evaluation passed"
    else
      echo "task-ecount evaluation failed" >&2
      eval_status=1
    fi
  else
    echo "pi completed without creating /work/ecount.xsh" >&2
    eval_status=1
  fi

  agents_sha256=$(sha256sum /work/agents.md | awk "{print \$1}")
  handbook_sha256=$(sha256sum /work/handbook.md | awk "{print \$1}")
  task_sha256=$(sha256sum /work/task-ecount.md | awk "{print \$1}")
  candidate_sha256=
  oracle_sha256=
  if test -f /session/candidate.stdout; then
    candidate_sha256=$(sha256sum /session/candidate.stdout | awk "{print \$1}")
  fi
  if test -f /session/oracle.stdout; then
    oracle_sha256=$(sha256sum /session/oracle.stdout | awk "{print \$1}")
  fi
  result=fail
  if test "$eval_status" -eq 0; then result=pass; fi
  printf "{\n  \"image_id\": \"%s\",\n  \"platform\": \"%s\",\n  \"provider\": \"%s\",\n  \"model\": \"%s\",\n  \"thinking\": \"%s\",\n  \"telemetry\": \"%s\",\n  \"offline\": \"%s\",\n  \"result\": \"%s\",\n  \"session\": \"/session/task-ecount-session.jsonl\",\n  \"inputs\": {\"agents_sha256\": \"%s\", \"handbook_sha256\": \"%s\", \"task_sha256\": \"%s\"},\n  \"outputs\": {\"candidate_sha256\": \"%s\", \"oracle_sha256\": \"%s\"}\n}\n" \
    "$GYM_IMAGE_ID" "$GYM_PLATFORM" "$PI_PROVIDER" "$PI_MODEL" "$PI_THINKING" \
    "$PI_TELEMETRY" "$PI_OFFLINE" "$result" "$agents_sha256" "$handbook_sha256" \
    "$task_sha256" "$candidate_sha256" "$oracle_sha256" > /session/run.json
  exit "$eval_status"
'
eval_status=$?

# --rm removes both containers. Named volumes are independent of --rm, so
# remove this dedicated volume only after the evaluator has copied its files.
"$docker_bin" volume rm "$session_volume" >/dev/null 2>&1 || true

if test "$agent_status" -ne 0; then
  exit "$agent_status"
fi
exit "$eval_status"

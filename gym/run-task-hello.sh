#!/bin/sh

set -u

docker_bin=${DOCKER:-docker}
platform=${PLATFORM:-linux/arm64}
gym_dir=${GYM_DIR:?GYM_DIR is required}
base_image=${BASE_IMAGE:?BASE_IMAGE is required}
session_volume=${SESSION_VOLUME:?SESSION_VOLUME is required}
work_dir=${WORK_DIR:?WORK_DIR is required}
output_dir=${OUTPUT_DIR:?OUTPUT_DIR is required}
pi_command=${PI_COMMAND:-pi}
pi_provider=${PI_PROVIDER:-openrouter}
pi_model=${PI_MODEL:-deepseek/deepseek-v4-flash-0731}
pi_thinking=${PI_THINKING:-high}
pi_telemetry=${PI_TELEMETRY:-0}
pi_offline=${PI_OFFLINE:-1}
pi_auth_file=${PI_AUTH_FILE:?PI_AUTH_FILE is required}
pi_agent_dir=${PI_AGENT_DIR:-/run/pi-agent}

mkdir -p "$work_dir" "$output_dir"
cp "$gym_dir/agents.md" "$gym_dir/handbook.md" "$gym_dir/task-hello.md" "$work_dir/"
rm -f \
  "$work_dir/answer.txt" \
  "$output_dir/session.jsonl" \
  "$output_dir/session.html" \
  "$output_dir/run.json"

test -f "$pi_auth_file" || {
  echo "Pi auth file does not exist: $pi_auth_file" >&2
  exit 2
}

# A fixed name makes the session location discoverable during a run. It is
# removed only after the evaluator has copied the session and manifest out.
"$docker_bin" volume rm "$session_volume" >/dev/null 2>&1 || true
"$docker_bin" volume create "$session_volume" >/dev/null

image_id=$("$docker_bin" image inspect --format '{{.Id}}' "$base_image")

set --
set -- "$@" --rm --platform "$platform"
set -- "$@" --read-only --tmpfs '/tmp:rw,noexec,nosuid,nodev'
set -- "$@" --tmpfs "$pi_agent_dir:rw,noexec,nosuid,nodev"
set -- "$@" --cap-drop=ALL --security-opt=no-new-privileges
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

agent_status=0
# The single-quoted script is evaluated inside the container, not by this host
# shell; its variables intentionally expand there.
# shellcheck disable=SC2016
"$docker_bin" run "$@" "$base_image" /bin/sh -eu -c '
  mkdir -p "$PI_CODING_AGENT_DIR"
  cp /run/pi-auth.json "$PI_CODING_AGENT_DIR/auth.json"
  chmod 600 "$PI_CODING_AGENT_DIR/auth.json"
  rm -f /session/task-hello-session.jsonl /session/task-hello-session.html
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
    --session /session/task-hello-session.jsonl \
    --print \
    @/work/task-hello.md \
    "Complete task-hello.md in /work. Run the required checks and leave the requested artifact there." &
  pi_pid=$!
  # Stream the session file as pi persists it: session entries are appended
  # and flushed synchronously, so tail -f shows each entry as it is written.
  # The file appears only once the first assistant message lands, so wait for
  # it (bounded) before starting the tail.
  retries=0
  while test ! -f /session/task-hello-session.jsonl && test "$retries" -lt 3000; do
    retries=$((retries + 1))
    sleep 0.1
  done
  if test -f /session/task-hello-session.jsonl; then
    tail -f /session/task-hello-session.jsonl &
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
  if test -f /session/task-hello-session.jsonl; then
    "$PI_COMMAND" --export /session/task-hello-session.jsonl /session/task-hello-session.html || true
  fi
  exit "$agent_status"
'
agent_status=$?

# Evaluate on the host: /work is a host bind mount (work_dir).
eval_status=0
if test -f "$work_dir/answer.txt"; then
  content=$(tr -d "\r\n" < "$work_dir/answer.txt")
  if test "$content" = "hello"; then
    echo "task-hello evaluation passed"
  else
    echo "task-hello evaluation failed: unexpected content" >&2
    eval_status=1
  fi
else
  echo "pi completed without creating /work/answer.txt" >&2
  eval_status=1
fi

# Copy the session out of the named volume, then record the run manifest.
cid=$("$docker_bin" create -v "$session_volume":/session "$base_image" true)
"$docker_bin" cp "$cid":/session/task-hello-session.jsonl "$output_dir/session.jsonl" 2>/dev/null || true
"$docker_bin" cp "$cid":/session/task-hello-session.html "$output_dir/session.html" 2>/dev/null || true
"$docker_bin" rm "$cid" >/dev/null 2>&1 || true

answer_sha256=
if test -f "$work_dir/answer.txt"; then
  answer_sha256=$(sha256sum "$work_dir/answer.txt" | awk '{print $1}')
fi
task_sha256=$(sha256sum "$gym_dir/task-hello.md" | awk '{print $1}')
result=fail
if test "$eval_status" -eq 0; then result=pass; fi
printf "{\n  \"image_id\": \"%s\",\n  \"platform\": \"%s\",\n  \"provider\": \"%s\",\n  \"model\": \"%s\",\n  \"thinking\": \"%s\",\n  \"telemetry\": \"%s\",\n  \"offline\": \"%s\",\n  \"result\": \"%s\",\n  \"session\": \"/session/task-hello-session.jsonl\",\n  \"inputs\": {\"task_sha256\": \"%s\"},\n  \"outputs\": {\"answer_sha256\": \"%s\"}\n}\n" \
  "$image_id" "$platform" "$pi_provider" "$pi_model" "$pi_thinking" \
  "$pi_telemetry" "$pi_offline" "$result" "$task_sha256" "$answer_sha256" > "$output_dir/run.json"

# --rm removes the agent container. Named volumes are independent of --rm, so
# remove this dedicated volume only after copying its files out.
"$docker_bin" volume rm "$session_volume" >/dev/null 2>&1 || true

if test "$agent_status" -ne 0; then
  exit "$agent_status"
fi
exit "$eval_status"

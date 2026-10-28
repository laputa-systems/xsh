set -eu
xsh=$1
service=$2
state=$3
log=$4
err=$5
client=$6
mode=$7
shift 7

scratch="$state/devices"
mkdir -p "$scratch"
mount --make-rprivate / 2>/dev/null || true
for node in null zero full random urandom tty; do
    : > "$scratch/$node"
    mount --bind "/dev/$node" "$scratch/$node"
done
mount -t tmpfs -o mode=0755,size=1m tmpfs /dev
for node in null zero full random urandom tty; do
    : > "/dev/$node"
    mount --bind "$scratch/$node" "/dev/$node"
done

"$xsh" "$service" --state "$state" >>"$log" 2>>"$err" &
service_pid=$!
attempts=0
until [ -S /dev/log ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -gt 1200 ]; then
        echo "scene: the service never published /dev/log" >>"$err"
        kill -KILL "$service_pid" 2>/dev/null || true
        exit 90
    fi
    sleep 0.05
done

case "$mode" in
basic | multiline | long | forge)
    "$client" "$mode"
    sleep 0.5
    ;;
count)
    "$client" count "$1"
    sleep 1
    ;;
stalled)
    kill -STOP "$service_pid"
    "$client" count "$1" &
    client_pid=$!
    sleep 1
    if kill -0 "$client_pid" 2>/dev/null; then
        echo "client-blocked" >>"$err"
    fi
    kill -CONT "$service_pid"
    wait "$client_pid"
    echo "client-finished" >>"$err"
    sleep 1
    ;;
*)
    echo "scene: unknown mode $mode" >>"$err"
    exit 91
    ;;
esac

kill -TERM "$service_pid"
status=0
wait "$service_pid" || status=$?
echo "service-status=$status" >>"$err"
test -e /dev/log && echo "socket-left-behind" >>"$err"
exit 0

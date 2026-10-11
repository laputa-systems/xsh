set -eu
# POSIX shells have no JSON values; this shared Python helper is timed explicitly.
exec /usr/bin/python3 -I "$2" "$1"

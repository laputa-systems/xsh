set -eu
index=0
while [ "$index" -lt 128 ]; do
    /usr/bin/printf 'probe %s\n' "$index"
    index=$((index + 1))
done

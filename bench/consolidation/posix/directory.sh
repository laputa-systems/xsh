set -eu
cd "$1/tree"
# Separate checked producers keep find/sort failures out of pipeline subshells.
/usr/bin/find . -mindepth 1 -print > "$TMPDIR/paths"
LC_ALL=C /usr/bin/sort "$TMPDIR/paths" > "$TMPDIR/sorted"
while IFS= read -r path; do
    if [ -L "$path" ]; then kind=link
    elif [ -d "$path" ]; then kind=dir
    elif [ -f "$path" ]; then kind=file
    else printf 'unexpected file kind: %s\n' "$path" >&2; exit 1
    fi
    printf '%s\t%s\n' "${path#./}" "$kind"
done < "$TMPDIR/sorted"
/bin/rm "$TMPDIR/paths" "$TMPDIR/sorted"

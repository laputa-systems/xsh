#!/bin/sh
# Run the pinned BusyBox testsuite against staged XSH applets.
#
#   UUTILS_ROOT=../ref/uutils-coreutils dev/compat/run-busybox.sh [UTILITY...]
#
# With no utility arguments, run every pinned BusyBox applet suite supported by
# the stage. A selected invocation writes only those utility rows to the report.
set -eu

repo=$(cd "$(dirname "$0")/../.." && pwd)
uutils=${UUTILS_ROOT:-$repo/../ref/uutils-coreutils}
uutils=$(cd "$uutils" && pwd)
mkdir -p "$repo/.work"
lock=${UUTILS_SUITE_LOCK:-$repo/.work/uutils-suite.lock}
exec 9>"$lock"
flock 9
# BusyBox tests create child processes; close the shared lock in each one.
mkdir -p "$repo/.work/tmp/busybox"
tmpdir=$(mktemp -d "$repo/.work/tmp/busybox/run.XXXXXX")
chmod 700 "$tmpdir"
export TMPDIR=$tmpdir TMP=$tmpdir TEMP=$tmpdir
work=
busybox_bin=
cleanup() {
	[ -z "$work" ] || rm -rf "$work"
	[ -z "$busybox_bin" ] || rm -rf "$busybox_bin/runtest-tempdir-links"
	rm -rf "$tmpdir"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

stage=${XSH_COMPAT_STAGE:-$repo/.work/compat-busybox-stage}
results=${COMPAT_RESULTS_DIR:-$repo/.work/compat-results/busybox}
report=${BUSYBOX_REPORT:-$results/busybox.json}
logs=${BUSYBOX_LOG_DIR:-$results/busybox-logs}
work_root=${BUSYBOX_WORK_ROOT:-$repo/.work/compat-work/busybox}
xsh=${XSH_BIN:-$repo/target/release/xsh}
lockfile=$repo/dev/compat/upstream.lock.json

busybox_source_root=${BUSYBOX_SOURCE_ROOT:-$repo/.work/upstream/busybox}

want_uutils=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["uutils"]["commit"])' "$lockfile")
have_uutils=$(git -C "$uutils" rev-parse HEAD)
if [ "$want_uutils" != "$have_uutils" ]; then
	echo "UUTILS_ROOT is at $have_uutils; dev/compat/upstream.lock.json pins $want_uutils" >&2
	exit 2
fi

busybox_version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["busybox"]["version"])' "$lockfile")
busybox_url=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["busybox"]["archive"])' "$lockfile")
busybox_sha=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["busybox"]["archive_sha256"])' "$lockfile")
busybox_source_name=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["busybox"]["source_directory"])' "$lockfile")
busybox_tmp=$busybox_source_root
busybox_archive=$busybox_tmp/busybox-$busybox_version.tar.gz
busybox_source=$busybox_tmp/$busybox_source_name
mkdir -p "$busybox_tmp"
if [ ! -f "$busybox_archive" ]; then
	python3 - "$busybox_url" "$busybox_archive" <<'PY'
import shutil
import sys
import urllib.request
from pathlib import Path

url, destination = sys.argv[1:]
destination = Path(destination)
partial = destination.with_name(destination.name + ".partial")
try:
    with urllib.request.urlopen(url, timeout=30) as response, partial.open("wb") as output:
        shutil.copyfileobj(response, output)
    partial.replace(destination)
finally:
    partial.unlink(missing_ok=True)
PY
fi
actual_sha=$(sha256sum "$busybox_archive" | cut -d ' ' -f 1)
if [ "$actual_sha" != "$busybox_sha" ]; then
	echo "BusyBox archive SHA-256 is $actual_sha; lock requires $busybox_sha" >&2
	exit 2
fi
if [ ! -x "$busybox_source/testsuite/runtest" ]; then
	tar -C "$busybox_tmp" -xf "$busybox_archive"
fi
if [ ! -x "$busybox_source/testsuite/runtest" ]; then
	echo "BusyBox testsuite missing at $busybox_source" >&2
	exit 2
fi

python3 "$repo/dev/compat/stage.py" --stage "$stage" --xsh "$xsh"
busybox_bin=$stage/busybox-bin
mkdir -p "$busybox_bin"
cp "$repo/dev/compat/xsh-busybox" "$busybox_bin/busybox"
chmod 755 "$busybox_bin/busybox"
cp "$uutils/.busybox-config" "$busybox_bin/.config"
python3 - "$stage/applets.json" "$stage/busybox-applets.txt" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text())
names = {entry["name"] for entry in manifest["applets"]}
names.update(entry["name"] for entry in manifest["aliases"])
Path(sys.argv[2]).write_text(", ".join(sorted(names)) + "\n")
PY

selection=$stage/busybox-selection
if [ "$#" -eq 0 ]; then
	python3 - "$busybox_source/testsuite" "$stage/applets.json" >"$selection" <<'PY'
import json
import sys
from pathlib import Path

tests = Path(sys.argv[1])
manifest = json.loads(Path(sys.argv[2]).read_text())
names = {entry["name"] for entry in manifest["applets"]}
names.update(entry["name"] for entry in manifest["aliases"])
tested = {
    path.name if path.is_dir() else path.stem
    for path in tests.iterdir()
    if (path.is_dir() or path.suffix == ".tests")
    and (path.name if path.is_dir() else path.stem) in names
}
for name in sorted(tested):
	print(name)
PY
else
	: >"$selection"
	for utility in "$@"; do
		case $utility in
			"" | *[!a-zA-Z0-9_.+-]*) echo "invalid BusyBox applet '$utility'" >&2; exit 2 ;;
		esac
		if [ ! -x "$stage/bin/$utility" ] || { [ ! -d "$busybox_source/testsuite/$utility" ] && [ ! -f "$busybox_source/testsuite/$utility.tests" ]; }; then
			echo "BusyBox test applet '$utility' is unavailable in XSH or the pinned suite" >&2
			exit 2
		fi
		printf '%s\n' "$utility" >>"$selection"
	done
fi

mkdir -p "$logs" "$results" "$work_root"
work=$(mktemp -d "$work_root/run.XXXXXX")
chmod 700 "$work"
cp -a "$busybox_source/testsuite" "$work/testsuite"
if [ -e "$work/testsuite/busybox.tests" ]; then
	mv "$work/testsuite/busybox.tests" "$work/testsuite/busybox.tests-"
fi
run_uid=${BUSYBOX_RUN_UID:-1000}
run_gid=${BUSYBOX_RUN_GID:-$run_uid}
if [ "$(id -u)" -eq 0 ]; then
	getent passwd "$run_uid" >/dev/null || { echo "no BusyBox test account for uid $run_uid" >&2; exit 2; }
	chown -R "$run_uid:$run_gid" "$busybox_bin" "$work" "$tmpdir"
fi
chmod 755 "$busybox_bin"

manifest=$work/run-manifest.tsv
: >"$manifest"
while IFS= read -r utility; do
	[ -n "$utility" ] || continue
	log=$logs/$utility.log
	set +e
	if [ "$(id -u)" -eq 0 ]; then
		(
			cd "$work/testsuite"
			setpriv --reuid="$run_uid" --regid="$run_gid" --clear-groups -- \
				env LC_ALL=C TZ=UTC HOME=/home/compat TMPDIR="$tmpdir" TMP="$tmpdir" TEMP="$tmpdir" \
					PATH="$busybox_bin:/usr/local/bin:/usr/bin:/bin" \
					XSH_COMPAT_STAGE="$stage" bindir="$busybox_bin" \
					tsdir="$work/testsuite" \
					timeout -sKILL "${BUSYBOX_TIMEOUT:-600s}" \
					sh ./runtest -v "$utility" 9>&-
		) >"$log" 2>&1
	else
		(
			cd "$work/testsuite"
			env LC_ALL=C TZ=UTC HOME=${HOME:-/home/compat} TMPDIR="$tmpdir" TMP="$tmpdir" TEMP="$tmpdir" \
				PATH="$busybox_bin:/usr/local/bin:/usr/bin:/bin" \
				XSH_COMPAT_STAGE="$stage" bindir="$busybox_bin" \
				tsdir="$work/testsuite" \
				timeout -sKILL "${BUSYBOX_TIMEOUT:-600s}" \
				sh ./runtest -v "$utility" 9>&-
		) >"$log" 2>&1
	fi
	status=$?
	set -e
	printf '%s\t%s\t%s\n' "$utility" "$status" "$log" >>"$manifest"
done <"$selection"

set +e
python3 "$repo/dev/compat/busybox_report.py" --report "$report" \
	--version "$busybox_version" --archive-sha256 "$busybox_sha" \
	--run-manifest "$manifest"
status=$?
set -e
exit "$status"

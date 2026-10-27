import errno
import hashlib
import os
import stat
import subprocess
import sys


def git_environment():
    return {**{name: value for name, value in os.environ.items() if not name.startswith("GIT_")},
            "GIT_OPTIONAL_LOCKS": "0", "GIT_NO_REPLACE_OBJECTS": "1"}


def git(root, *args, timeout=None):
    command = [b"git", b"-C", root, b"-c", b"safe.directory=" + root,
               b"-c", b"core.excludesFile=/dev/null", *args]
    return subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          env=git_environment(), timeout=timeout, check=True).stdout


def records(data):
    if data and not data.endswith(b"\0"):
        raise ValueError("Git output is not NUL terminated")
    return data.split(b"\0")[:-1] if data else []


def fingerprint(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def add(digest, value):
    digest.update(len(value).to_bytes(8, "big"))
    digest.update(value)


def index_bytes(root):
    location = git(root, b"rev-parse", b"--path-format=absolute", b"--git-path", b"index")
    if not location.endswith(b"\n"):
        raise ValueError("Git index path is not terminated")
    path = location[:-1]
    try:
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    except FileNotFoundError:
        return b"\x00"
    with os.fdopen(descriptor, "rb") as source:
        before = os.fstat(source.fileno())
        if not stat.S_ISREG(before.st_mode):
            raise ValueError("Git index is not a regular file")
        data = source.read()
        if fingerprint(os.fstat(source.fileno())) != fingerprint(before):
            raise ValueError("Git index changed during inspection")
        if fingerprint(os.lstat(path)) != fingerprint(before):
            raise ValueError("Git index changed during inspection")
        return b"\x01" + data


def capture(root=None):
    root = os.fsencode(os.path.realpath(os.getcwd() if root is None else root))
    top = git(root, b"rev-parse", b"--show-toplevel")
    if not top.endswith(b"\n") or os.path.realpath(top[:-1]) != root:
        raise ValueError("source repository root is unavailable")
    state = git(root, b"status", b"--porcelain=v2", b"--branch", b"-z", b"--untracked-files=all")
    index = git(root, b"ls-files", b"--stage", b"-z", b"--")
    gitlinks = {entry.split(b"\t", 1)[1] for entry in records(index)
                if entry.startswith(b"160000 ") and b"\t" in entry}
    index_details = git(root, b"ls-files", b"--debug", b"-z", b"--")
    raw_index = index_bytes(root)
    names = git(root, b"ls-files", b"--cached", b"--others", b"--exclude-standard", b"-z", b"--")
    source_names = set(records(names))
    digest = hashlib.sha256()
    add(digest, state)
    add(digest, index)
    add(digest, index_details)
    add(digest, raw_index)
    for name in sorted(source_names):
        name.decode("utf-8")
        if name.startswith(b"/") or any(part in (b"", b".", b"..") for part in name.split(b"/")):
            raise ValueError("Git returned an invalid source path")
        path = os.path.join(root, name)
        if os.path.commonpath((root, os.path.realpath(os.path.dirname(path)))) != root:
            raise ValueError("source ancestor escapes the repository")
        add(digest, name)
        try:
            before = os.lstat(path)
        except FileNotFoundError:
            if name in gitlinks:
                raise ValueError("source submodule is unavailable")
            add(digest, b"missing")
            continue
        add(digest, str(before.st_mode).encode("ascii"))
        if name in gitlinks:
            if not stat.S_ISDIR(before.st_mode):
                raise ValueError("source submodule is not a directory")
            add(digest, b"submodule")
            add(digest, capture(path).encode("ascii"))
        elif stat.S_ISLNK(before.st_mode):
            target = os.path.realpath(path)
            if os.path.commonpath((root, target)) != root:
                raise ValueError("source symlink escapes the repository")
            try:
                target_info = os.stat(path)
            except FileNotFoundError:
                target_info = None
            except OSError as error:
                if error.errno != errno.ELOOP:
                    raise
                target_info = None
            if (target_info is not None and stat.S_ISREG(target_info.st_mode)
                    and os.path.relpath(target, root) not in source_names):
                raise ValueError("unobserved symlink target")
            add(digest, b"link")
            add(digest, os.readlink(path))
        elif stat.S_ISREG(before.st_mode):
            flags = os.O_RDONLY | os.O_NOFOLLOW
            with os.fdopen(os.open(path, flags), "rb") as source:
                if fingerprint(os.fstat(source.fileno())) != fingerprint(before):
                    raise ValueError("source changed before inspection")
                add(digest, b"file")
                add(digest, hashlib.file_digest(source, "sha256").digest())
                if fingerprint(os.fstat(source.fileno())) != fingerprint(before):
                    raise ValueError("source changed during inspection")
        else:
            raise ValueError("unsupported source entry")
        if fingerprint(os.lstat(path)) != fingerprint(before):
            raise ValueError("source changed during inspection")
    if (state != git(root, b"status", b"--porcelain=v2", b"--branch", b"-z", b"--untracked-files=all")
            or index != git(root, b"ls-files", b"--stage", b"-z", b"--")
            or index_details != git(root, b"ls-files", b"--debug", b"-z", b"--")
            or raw_index != index_bytes(root)
            or names != git(root, b"ls-files", b"--cached", b"--others", b"--exclude-standard", b"-z", b"--")):
        raise ValueError("Git state changed during inspection")
    return digest.hexdigest()


def changed_paths(root):
    root = os.fsencode(os.path.realpath(root))
    for entry in records(git(root, b"ls-files", b"-v", b"-z", b"--")):
        if len(entry) < 3 or entry[1:2] != b" ":
            raise ValueError("Git returned an invalid index entry")
        tag = entry[0:1]
        flags = []
        if tag.islower():
            flags.append("assume-unchanged")
        if tag.upper() == b"S":
            flags.append("skip-worktree")
        if flags:
            raise ValueError(f"changed path discovery cannot inspect {os.fsdecode(entry[2:])!r}: index has {', '.join(flags)}")
    return b"".join(git(root, *args) for args in (
        (b"diff", b"--cached", b"--no-renames", b"--name-only", b"-z", b"--"),
        (b"diff", b"--no-renames", b"--name-only", b"-z", b"--"),
        (b"ls-files", b"--others", b"--exclude-standard", b"-z", b"--"),
    ))


if __name__ == "__main__":
    try:
        if sys.argv[1:] == ["--changed"]:
            sys.stdout.buffer.write(changed_paths(os.getcwd()))
        elif sys.argv[1:] == ["--common-dir"]:
            sys.stdout.buffer.write(git(os.fsencode(os.path.realpath(os.getcwd())), b"rev-parse",
                                        b"--path-format=absolute", b"--git-common-dir"))
        elif sys.argv[1:] in (["--commit"], ["--tree"]):
            ref = b"HEAD" if sys.argv[1] == "--commit" else b"HEAD^{tree}"
            sys.stdout.buffer.write(git(os.fsencode(os.path.realpath(os.getcwd())), b"rev-parse", ref))
        elif not sys.argv[1:]:
            print(capture())
        else:
            raise ValueError("unsupported source inspection mode")
    except (OSError, UnicodeError, ValueError, subprocess.SubprocessError) as error:
        print(f"source inspection failed: {error}", file=sys.stderr)
        raise SystemExit(2)

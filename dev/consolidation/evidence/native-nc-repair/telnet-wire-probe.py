import argparse
import socket
import subprocess
from pathlib import Path


def run_case(xsh: str, script: Path, label: str, flags: list[str], expect_eof: bool, expect_reply: bytes, requests: bytes, stdout_bytes: bytes) -> None:
    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(1)
    listener.settimeout(2)
    process = None
    peer = None
    try:
        process = subprocess.Popen(
            [xsh, str(script), *flags, "127.0.0.1", str(listener.getsockname()[1])],
            cwd=script.parent.parent,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        process.stdin.close()
        peer, _ = listener.accept()
        peer.settimeout(0.5)
        try:
            eof_before_telnet = peer.recv(1) == b""
        except socket.timeout:
            eof_before_telnet = False
        peer.sendall(requests)
        reply = bytearray()
        timed_out = False
        try:
            while len(reply) < len(expect_reply):
                chunk = peer.recv(len(expect_reply) - len(reply))
                if not chunk:
                    break
                reply.extend(chunk)
        except socket.timeout:
            timed_out = True
        peer.close()
        peer = None
        try:
            status = process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.terminate()
            try:
                status = process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                process.kill()
                status = process.wait(timeout=1)
            timed_out = True
        stdout = process.stdout.read()
        stderr = process.stderr.read()
        print(f"{label}: eof_before_telnet={eof_before_telnet} reply={bytes(reply).hex() or 'empty'} timed_out={timed_out} status={status} stdout={stdout.hex()} stderr={stderr!r}")
        if eof_before_telnet != expect_eof or bytes(reply) != expect_reply or timed_out or status != 0 or stdout != stdout_bytes or stderr:
            raise SystemExit(f"{label} did not match its expected wire behavior")
    finally:
        if peer is not None:
            peer.close()
        listener.close()
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=1)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--xsh", required=True)
    parser.add_argument("--script", required=True, type=Path)
    args = parser.parse_args()
    requests = bytes([255, 253, 1, 255, 251, 3])
    refusals = bytes([255, 252, 1, 255, 254, 3])
    run_case(args.xsh, args.script, "with -N", ["-t", "-n", "-N"], True, b"", requests, requests)
    run_case(args.xsh, args.script, "without -N", ["-t", "-n"], False, refusals, requests, requests)


if __name__ == "__main__":
    main()

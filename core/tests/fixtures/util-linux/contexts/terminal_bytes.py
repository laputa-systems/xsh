import os


for descriptor, payload in [(1, b"\x00\xff\x1b[31mout\x1b[0m\n"), (2, b"\x00\xfeerr\n")]:
    assert os.isatty(descriptor)
    width = os.get_terminal_size(descriptor).columns
    os.write(descriptor, f"width={width}\n".encode("ascii") + payload)
raise SystemExit(7)

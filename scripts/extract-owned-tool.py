import os
import shutil
import sys
import tarfile

archive_path, member_name, output_path = sys.argv[1:]
with tarfile.open(archive_path, "r:xz") as archive:
    member = archive.getmember(member_name)
    if not member.isfile():
        raise SystemExit(f"{member_name} is not a regular file")
    source = archive.extractfile(member)
    if source is None:
        raise SystemExit(f"could not read {member_name}")
    with source, open(output_path, "wb") as output:
        shutil.copyfileobj(source, output)
os.chmod(output_path, 0o755)

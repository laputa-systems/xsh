#!/bin/sh
set +e
unset LD_PRELOAD
printf '%s\n' 'image=sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c' 'source=787ba37a43e808c6b63b85b17cc912fe36352b22/tree31681c63f02b825b4352017482230a33e78a819d' 'runtime_preload=unset' 'cpu_cap=2' 'memory_cap=4g' 'swap=none' 'network=none' > /scratch/environment.txt
id -u > /scratch/root.uid
printf '%s\n' '/artifact/xsht test --exact core/tests/test-uu-rm.xsh::test_uu_rm_inaccessible_dir_interactive' > /scratch/root.argv.txt
TMPDIR=/scratch/root/tmp HOME=/scratch/root/home /artifact/xsht test --exact core/tests/test-uu-rm.xsh::test_uu_rm_inaccessible_dir_interactive > /scratch/root/stdout 2> /scratch/root/stderr
printf '%s\n' "$?" > /scratch/root/status
command -v setpriv > /scratch/setpriv.path
setpriv --reuid=1000 --regid=1000 --clear-groups /usr/bin/id -u > /scratch/compat.uid 2> /scratch/compat-id.stderr
printf '%s\n' '/artifact/xsht test --exact core/tests/test-uu-rm.xsh::test_uu_rm_inaccessible_dir_interactive' > /scratch/compat.argv.txt
TMPDIR=/scratch/compat/tmp HOME=/scratch/compat/home setpriv --reuid=1000 --regid=1000 --clear-groups /artifact/xsht test --exact core/tests/test-uu-rm.xsh::test_uu_rm_inaccessible_dir_interactive > /scratch/compat/stdout 2> /scratch/compat/stderr
printf '%s\n' "$?" > /scratch/compat/status
exit 0

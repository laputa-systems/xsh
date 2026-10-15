from pathlib import Path
import subprocess
root=Path('/scratch'); names=[('curl-write','core/tests/test-curl.xsh::test_curl_write_failure_exits_23'),('curl-names','core/tests/test-curl.xsh::test_curl_output_names'),('curl-resume','core/tests/test-curl.xsh::test_curl_ranges_and_resume'),('awk-001','core/tests/test-awk.xsh::test_awk_matrix_streams_and_processes_shard_001'),('tail-wait','core/tests/test-gnu-tail.xsh::test_gnu_tail_wait_log')]
for label,name in names:
 argv=['/candidate/xsht','test','-j','1','--timeout','50s','--exact',name]
 (root/(label+'.argv')).write_text(repr(argv)+'\n')
 with (root/(label+'.stdout')).open('wb') as out,(root/(label+'.stderr')).open('wb') as err:
  r=subprocess.run(argv,stdout=out,stderr=err,timeout=70)
 (root/(label+'.exit')).write_text(str(r.returncode)+'\n')
 print(label,r.returncode,flush=True)
 if r.returncode: raise SystemExit(r.returncode)

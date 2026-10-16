set -eu
main=/home/josh/d/laputa-systems/xsh
snapshot=/home/josh/d/laputa-systems/xsh/.work/tmp/w9-controller-replay-56bf
exec 8< "$main/.work"
exec 9< "$main/.git/worktrees"
exec 10< "$snapshot"
cd /
mount --bind "$snapshot" "$main"
mount --no-canonicalize --bind "/proc/$$/fd/8" "$main/.work"
mount --no-canonicalize --bind "/proc/$$/fd/9" "$main/.git/worktrees"
cd "$main"
export GIT_OPTIONAL_LOCKS=0
printf "namespace_mnt=%s uid_gid=%s:%s groups=%s\n" "$(readlink /proc/self/ns/mnt)" "$(id -u)" "$(id -g)" "$(id -G)"
printf "controller_head=%s tree=%s\n" "$(git rev-parse HEAD)" "$(git rev-parse HEAD^{tree})"
printf "snapshot_root_inode=%s mounted_main_inode=%s\n" "$(stat -Lc %d:%i /proc/$$/fd/10)" "$(stat -Lc %d:%i "$main")"
printf "work_fd_inode=%s mounted_work_inode=%s\n" "$(stat -Lc %d:%i /proc/$$/fd/8)" "$(stat -Lc %d:%i "$main/.work")"
printf "worktrees_fd_inode=%s mounted_worktrees_inode=%s\n" "$(stat -Lc %d:%i /proc/$$/fd/9)" "$(stat -Lc %d:%i "$main/.git/worktrees")"
findmnt -no SOURCE,FSTYPE,PROPAGATION -T "$main"
findmnt -no SOURCE,FSTYPE,PROPAGATION -T "$main/.work"
findmnt -no SOURCE,FSTYPE,PROPAGATION -T "$main/.git/worktrees"
python3 /home/josh/d/laputa-systems/xsh/dev/consolidation/w9_publication_adapter_runner.py compare-derived-captures --candidate-root /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-56bf --candidate-source-receipt /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-56bf/.work/consolidation/source-cohort-w9-final-56bf.receipt --reference-source-root /home/josh/d/laputa-systems/xsh/.work/wt/w9-adapter-ref-e240 --derived-reference-source-root /home/josh/d/laputa-systems/xsh/.work/wt/w9-derived-api-imports --derived-reference-derivation-receipt /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/derived-reference-capture/derivation.receipt --original-capture-receipt /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/adapter-capture/reference/capture.receipt --reference-exact-build-receipt /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-56bf/.work/consolidation/w9-publication-adapter-v1-az717a85/w9-publication-adapter-reference-e240efad273f.receipt --candidate-exact-build-receipt /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-final-56bf/.work/consolidation/w9-publication-adapter-v1-az717a85/w9-publication-adapter-candidate-56bf3286c003.receipt --candidate-capture-receipt /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/adapter-capture/candidate/capture.receipt --reference-exact-capture-receipt /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/exact-reference/capture.receipt --candidate-exact-capture-receipt /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/exact-candidate/capture.receipt --assignment-facts-capture-receipt /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/assignment-facts/assignment-capture.receipt --output /home/josh/d/laputa-systems/xsh/.work/consolidation/w9-final-56bf/compare-r3

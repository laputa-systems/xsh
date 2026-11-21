# Guide selection rule

Fixed while the revision run was in progress and before any of its results were read.

The contenders are `xsh-baseline` and `xsh-candidate` from `dev-initial`, and
`xsh-revised` from `dev-revision`, each 12 development tasks × 2 samples. Rates are
task-clustered as in the report.

1. Comparable correctness: a contender qualifies when its correct-within-three-programs
   rate is at most 5 points below the best contender's.
2. Among qualifying contenders, the selected context has the lowest total tokens per
   solved task (all tokens of all its trials over its solved trials).
3. When two qualifying contenders are within 5% of each other on tokens per solved
   task, the one with the higher first-attempt correct rate wins; if that ties too, the
   smaller context wins.

Context size is not a criterion on its own. The selected file is copied byte for byte to
`contexts/selected.md` and frozen with its SHA-256 and measured token count before any
held-out task exists. If `xsh-baseline` wins, the current documentation is kept and the
report says the compact guide did not beat it.

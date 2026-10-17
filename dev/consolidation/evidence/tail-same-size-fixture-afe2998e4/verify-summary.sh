set -euo pipefail
summary=${1:?usage: bash verify-summary.sh SUMMARY.tsv}
awk -F '\t' 'NF != 2 || $1 != 0 { bad=1 } END { if (NR != 3) bad=1; exit bad }' "$summary"
printf 'three selected native tests exited 0\n'

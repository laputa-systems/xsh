Create one file named ecount.xsh in the task working directory.

The program must accept one root directory argument and print a count of file
name extensions below that root. It must traverse the filesystem directly
through XSH APIs. It must not start subprocesses or invoke an external command.

The behavior is defined by this oracle command:

    fd  --color=never -tf . '/usr/share' | awk -F. 'NF > 1 {print tolower($NF)}' | sort | uniq -c | sort -n

The evaluator invokes the candidate equivalently to:

    xsh ecount.xsh '/usr/share'

Acceptance is a byte-for-byte stdout match with the oracle output, including
leading count padding, tie ordering, and the final newline. Both commands
must complete successfully. The evaluator supplies the oracle comparison; no
verification harness needs to be added to the task.

The candidate must therefore:

- visit the same regular-file set as the oracle;
- omit paths whose complete path string has no period;
- for paths with a period, use the final period-separated field lowercased;
- count equal resulting strings;
- sort output as the oracle does: numeric count ascending with the oracle’s
  tie behavior;
- emit only the result lines;
- accept the root as an argument rather than baking the current result or
  repository contents into the program.

The solution must not use run, process APIs, spawn, shell commands, or any
other subprocess boundary. Do not hard-code the current extension list or
counts. Do not modify files below the root while computing the result.

Use the handbook and xsht api as the available XSH reference. A normal
development loop is:

    xsht check ecount.xsh
    xsht fmt ecount.xsh
    xsht lint ecount.xsh
    xsh ecount.xsh '/usr/share'

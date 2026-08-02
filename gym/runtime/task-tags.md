Create one file named `tag.xsh` in the task working directory.

The program must accept zero or more command-line arguments and print exactly
one line with this shape:

    tags: <lowercase argument 1>, <lowercase argument 2>, ...

Lowercase each argument independently, preserve spaces inside an argument, and
separate arguments with the two-character separator `, `. With no arguments,
print `tags:` followed by the final newline. The evaluator supplies several
argument lists, so do not hard-code one result.

The program must perform the transformation through XSH values. It must not
start subprocesses, invoke an external command, or add diagnostic text to
stdout.

Use the handbook and the available `xsht` checks as the reference. A normal
development loop is:

    xsht check tag.xsh
    xsht fmt tag.xsh
    xsht lint tag.xsh
    xsh tag.xsh Alpha "Two Words" BETA

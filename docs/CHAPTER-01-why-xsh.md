XSH is a clean-slate systems scripting language for a modern Linux userspace.

It is not a POSIX shell replacement in the compatibility sense, and it is not
an interactive terminal interface. It is a language for writing the glue that
holds a system together: package managers, build recipes, init systems, service
supervisors, installer scripts, maintenance tools, and distribution policy.

## The Archaeological Site

The modern Linux userspace is an archaeological site. Beneath every build lies
sedimentary layers of languages accumulated over decades: shell scripts call m4
macros, configure scripts emit Makefiles, Makefiles invoke compilers through
wrapper scripts, and those wrappers are often written in Perl, Python, awk, sed,
or a private DSL that nobody meant to become infrastructure.

This is Unix sludge: the entropic product of many weak languages duct-taped
together because no single glue language was powerful enough for the whole job.

XSH starts from a different premise. The system deserves one strong language for
glue, one that can speak fluently to processes, files, paths, byte streams,
structured data, and system state without turning every boundary into a quoting
puzzle.

## What Shell Got Right

The old Unix shell succeeded because it made operating-system pieces feel
composable. Processes, files, pipes, environment variables, working
directories, exit statuses, and argument vectors became everyday building
blocks. A small script could assemble existing programs into something larger
than any one of them.

That idea is still right.

XSH keeps the useful model: coarse-grained reuse, explicit process boundaries,
pipeline-shaped data flow, ordinary source files, and scripts that can grow into
tools. It treats the Unix process model as an asset, not as historical baggage.
The expensive work should be visible as a process, a file operation, or a typed
host API, not hidden behind a scheduler or runtime trick.

## What Shell Got Wrong

The traditional shell encoded too much composition in strings and ambient state.
It made parsing dynamic, word splitting implicit, quoting fragile, error flow
surprising, and standards vague enough that every serious script eventually
became a local dialect.

XSH rejects that sludge:

- no implicit eval;
- no hidden word splitting;
- no untyped text as the only interface between programs;
- no ad hoc DSL stacking where a script generates another language to generate
  another language;
- no pretending that decades of compatibility quirks are a design philosophy.

The goal is not to preserve the old spellbook. The goal is to carry forward the
part of Unix that was worth preserving.

## One Glue Language

XSH is not trying to be a general-purpose application language. It is trying to
be the best possible language for orchestration: starting processes, shaping
argv, moving through directories, reading and writing files, transforming text
and bytes, crossing JSON boundaries, inspecting host state, and making expected
failures visible.

Shell is the language of heterogeneity. It must speak to everything. XSH says
that heterogeneity should be handled with clarity rather than incantation.

For old Unix hands, the promise is familiar: small pieces, composed well. The
difference is that XSH gives that promise a modern type system, structured
errors, typed paths, structured streams, and a runtime that can trace what
happened.

That is the worthy successor: not a clone of the old shell, and not a small
application runtime wearing shell syntax, but a clean language for the work the
old shell proved was essential.

## What XSH Is Not

XSH does not compete with runtimes designed for fine-grained concurrency,
long-lived application services, or interactive terminal interfaces. Keep those
jobs in a service runtime, a dedicated TUI framework, or a specialized tool;
use XSH to compose the host-facing work around them.


# Deferred stress inputs

Use these only at the final performance check, after functional implementation,
annotation removal, migration, and consolidation are complete. No scaling run
or measurement collection is an implementation prerequisite.

The existing deterministic generator covers aliases, instantiation, wide rows,
nested containers, recursive effects, module diamonds, overloads, and
adversarial inputs. Generate a selected case into a fresh temporary directory:

```sh
python3 bench/typing/scaling.py generate --case module_diamonds-1000-inferred --output /tmp/xsh-scaling-case
```

The manifest records available cases and reproducible source identities; its
historical timing/scaling thresholds are not current campaign prerequisites.
Use these inputs to investigate a concrete performance problem rather than
collecting exhaustive reports.

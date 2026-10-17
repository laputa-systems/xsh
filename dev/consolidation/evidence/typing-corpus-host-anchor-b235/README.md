# Joint corpus after the host-anchor repair

This continuation composes compiler source `92248b13172d809ce38fbced5f606e6c0a719709`
into the isolated corpus at `be4751b83e60182b2d00eba31772a98cbc1a81a6`.
The [repair qualification](../typing-host-anchor-b235/late-anchors/README.md)
owns the focused Rust, warnings-denied production build and execution proof.
This lane makes no compiler or standard-library edits and runs no build.

The program cohort, 9,194 AST sites and frozen categories remain those of
`a8d5bd910`. The prior [613-removal result](../typing-corpus-b235/README.md)
remains unchanged. Only restoration choices change.

## Qualified products

Both views use the read-only pair built from compiler source `92248b131`:

| Product | SHA-256 |
|---|---|
| xsh | `96fef96bb1443ffec2d621bb14c2d693e930ad1511d92aa6d8fa00a55e0304ef` |
| xsht | `687ef44beda59d9395ccaacc35b63c247bc906629602e796f9fd24351ef44cb6` |

`check.sh` and `run-witnesses.sh` bind this exact pair. Checks use the pinned
`3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`
Linux musl image, four CPUs, 8 GiB memory, no extra swap, no network and UID 1000.
The original program sources remain XSH `ddc17d3d53c819c343f9ecaa738ac67d59e9033a`
and Laputa `ca658ce1da9d89c55209c25268dfdf687ff4fdc6`.

## Joint result

All eleven annotated roots pass with the new products. Retrying thirteen sites
from the prior host-operation restoration groups produces a 626-removal
candidate. XSH rejects it during indexed lowering in `longest_line`,
`parse_proc_stat_with_policy` and `traced_syscall`. The failed candidate has no
removal percentage. `retry-sites.tsv` records each site's final treatment.

The passing joint candidate removes 615 annotations:

| Eligible category | Frozen denominator | Removed | Restored | Rate |
|---|---:|---:|---:|---:|
| Local | 693 | 236 | 457 | 34.05% |
| Internal parameter, return and effect | 2,190 | 379 | 1,811 | 17.31% |

The additional two removals are `zone_suffix`'s input and return annotations.
They are removed together with the prior 613 sites. All ten XSH roots and the
Laputa root pass ordinary checking and indexed lowering. The final AST-span
validation covers all 48 loaded files and protects every retained, schema and
exported-interface span. No cast, validation call, algorithm, effect contract or
caller training is inserted.

The 95% local and 85% internal targets remain unmet. Restoration stays
conservative by failed declaration group. This result does not claim that each
restored annotation is individually necessary or that the subset is maximal.
All restored sites stay eligible.

## Ambiguous indirect receiver witness

The original direct-anchor split witness now passes, including removal of its
return annotation. The adjacent raw stderr files retain those checks. That
repair remains valid because `anchor(text)` directly establishes Str for the
input parameter.

The `indirect-anchor` pair changes only the parameter annotation. Its annotated
view passes. Its reduced view declares `flat` through `text.replace`, then
passes `flat` to a known Str helper before nested splits. The registry defines
two applicable effect-free operations for that call:

| Receiver | Positional argument | Labeled argument | Return | Registry line |
|---|---|---|---|---:|
| Str | `from: Str` | `with: Str` | Str | 717 |
| Regex | `text: Str` | `with: Str` | Str | 1292 |

Both signatures are in `crates/xsh-registry/src/signature/methods.rs`.
`anchor(flat)` establishes Str for the result. It cannot distinguish the Str
and Regex receivers. The caller must not train the declaration's input type.
This witness therefore establishes a genuine ambiguity, not a confirmed missing
anchor or a requirement that the reduced program pass.

The actual reduced diagnostic remains `compact.indexed-build` at `line.split`.
That diagnostic does not identify the two receiver alternatives. The raw output,
current rejection and eligible restoration denominator remain unchanged. The
named generic-body restriction remains a separate coverage boundary.

Further shared host-obligation investigation uses genuinely discriminating
contexts. It must preserve this ambiguous witness as a negative control. This
classification correction runs no checks and changes no compiler or fixture.

## Pending behavior gate

The strict named 26 executions remain prepared and unrun. The native runner uses
this new product pair and the final joint source view. Each of thirteen existing
behavior witnesses must actually select and pass once in each view, with zero
failures or skips. The shared native lock, immutable-view checks, product guards
and exact-container cleanup remain required. No general inference, main merge,
resource or performance acceptance is claimed.

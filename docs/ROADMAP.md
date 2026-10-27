# XSH roadmap

Work proceeds in this order:

1. **Pyramid:** complete scoped verification, including accurate selection,
   results, source and product binding, resource accounting, and representative
   workloads. See [the Pyramid campaign](../pyramid-campaign.md).
2. **Inference:** complete general type, row, and effect inference and its
   compiler and corpus acceptance. See [the inference campaign](../full-inference-campaign.md).
3. **Lean:** establish a connected compiler proof and execution correspondence,
   then continue the full-core obligations. See [the Lean campaign](../xsh-lean.md).

Pyramid starts on the current source; it does not wait for inference to finish.
When inference changes a Pyramid input or contract, rerun the affected checks.
Lean's source-bound correspondence uses the qualified source available after
the preceding work. Model proofs, finite witnesses, and Rust correspondence
remain distinct evidence.

The Deser cutover remains in place. Its performance thresholds are waived;
outstanding correctness and platform qualification are not passes. See
[the Deser campaign](../campaign-deser.md).

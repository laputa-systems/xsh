Frozen language source-contract audit: 125 rows

Actual-source contract completeness review of exactly frozen125 non-structured-stage language rows. Source references, owned focused results and owner log confirmations are distinguished; not full Gate C or runtime preparation acceptance.

Owned executed selectors: compound4/4, Eq/Ne4/4, source15/15, permission25/25. Owner exact source logs: iteration22/22, constructor17/17, final call-contract9/9 (including outgoing transport and lexical loop controls). Counts describe bounded test families, never 125 acceptance passes.

| Row | Applicability | Concrete | Negative | Forwarded | Effects | Retained fact |
|---|---|---|---|---|---|---|
| language.assertion | fixed_domain_or_explicit_syntax | reference | owned executed axis | not applicable | reference | owned executed axis |
| language.assignment.Add | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.assignment.Div | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.assignment.Mul | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.assignment.Rem | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.assignment.Set | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.assignment.Sub | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Add.duration | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Add.float | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Add.integer | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Add.list | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Add.text | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.And | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Div.duration_ratio | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Div.duration_scale | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Div.float | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Div.integer | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Eq | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Ge | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Gt | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.In.Bytes | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.In.EnvPathList | generalized_relation_applicable | owned executed axis | owned executed axis | reference | owned executed axis | owned executed axis |
| language.binary.In.List | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.In.Map | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.In.Path | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.In.Record | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.In.Str | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Le | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.binary.Lt | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.binary.Mul.duration_scale | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Mul.duration_scale_reverse | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Mul.float | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Mul.integer | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Ne | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.NotIn.Bytes | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.NotIn.EnvPathList | generalized_relation_applicable | owned executed axis | owned executed axis | reference | owned executed axis | owned executed axis |
| language.binary.NotIn.List | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.NotIn.Map | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.NotIn.Path | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.NotIn.Record | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.NotIn.Str | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Or | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Rem.integer | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.ResultFallback.Optional | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | reference | owned executed axis |
| language.binary.ResultFallback.Result | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | reference | owned executed axis |
| language.binary.Sub.duration | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Sub.float | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.binary.Sub.integer | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.call.checked_alias | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.call.direct_named | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.call.erased_Proc | fixed_explicit_erased_callable_boundary | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.call.erased_Pure | fixed_explicit_erased_callable_boundary | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.call.module_contract | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.call.named_spread | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.call.rest_and_splice | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.call.stage_descriptor | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.command.cd | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.command.env | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.command.eprint | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.command.print | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.comparison_chain | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.comprehension.List | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.comprehension.Map | generalized_relation_applicable | owner GREEN axis | reference | owner GREEN axis | reference | owner GREEN axis |
| language.constructor.Err | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.constructor.Ok | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.constructor.Path | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.constructor.error_variant | explicit_declaration_owned_template | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.constructor.range | fixed_domain_or_explicit_syntax | reference | reference | not applicable | reference | reference |
| language.constructor.record | explicit_declaration_owned_template | reference | reference | reference | reference | reference |
| language.constructor.tag | explicit_declaration_owned_template | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.default.callable | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.default.record | explicit_declaration_owned_template | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.descriptor.cli | explicit_dynamic_boundary | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.dynamic.membership | explicit_dynamic_boundary | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.dynamic.operators | explicit_dynamic_boundary | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.dynamic.pipeline | explicit_dynamic_boundary | owned executed axis | owned executed axis | not applicable | owned executed axis | owned executed axis |
| language.dynamic.projection | explicit_dynamic_boundary | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.index.List | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.index.Map | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.iteration.Bytes | fixed_domain_or_explicit_syntax | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.Bytes.Result | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.List | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.List.Result | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.Map | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.Map.Result | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.Str | fixed_domain_or_explicit_syntax | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.Str.Result | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.iteration.Stream | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.iteration.Stream.Result | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.literal.List | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.literal.Map | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.literal.Record | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.Bool | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.Bytes | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.Duration | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.Int | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.Path | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.Str | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.map_key.UInt | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.pattern.test | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.postfix.optional | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.postfix.propagate | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.projection.constant_key | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.projection.field | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.record.update | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.require.contextual | explicit_dynamic_boundary | reference | reference | not applicable | reference | reference |
| language.require.explicit | explicit_dynamic_boundary | reference | reference | reference | reference | reference |
| language.run.CaptureBytes | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.CaptureBytesRecord | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.CaptureText | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.CaptureTextRecord | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.Plain | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.Status | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.StreamBytes | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.run.StreamText | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.slice.Bytes | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.slice.List | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.slice.Str | fixed_domain_or_explicit_syntax | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.spawn | fixed_domain_or_explicit_syntax | reference | reference | not applicable | reference | reference |
| language.unary.Neg | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.unary.Not | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | owned executed axis | owned executed axis |
| language.wait | generalized_relation_applicable | reference | reference | reference | reference | reference |
| language.yield | generalized_relation_applicable | owned executed axis | owned executed axis | owned executed axis | reference | owned executed axis |
| language.yield_delegation.List | generalized_relation_applicable | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis | owner GREEN axis |
| language.yield_delegation.Stream | generalized_relation_applicable | reference | reference | reference | reference | reference |

All previously missing inspected row-specific witness axes now have exact source references. No actual source exception remains in the bounded families executed or refreshed here. The final lexical-loop compatibility witness remains auxiliary and does not expand the frozen 125-row inventory. This establishes source evidence completeness, not immutable final Gate C acceptance: owned execution, actual owner GREEN logs and inspected references remain distinct. Runtime preparation/execution remains outside scope.

Capture contract qualification: the exact postfix and six legal channels retain ENV with original failure operands and Project(ResultError), and missing ENV refuses. Implicit ResultList failure transport uses baseline coarse Error with empty producer profile; precise Stream E rejection is preserved. That corrected authored expectation is not recorded as a production widening fix.

JSON preserves the frozen operation hash, current inspected source hashes, exact qualified domain/effect evidence, applicable axes without owned execution, and historical RED-to-GREEN receipts.

Final source checkpoint: the final nine-test call-contract family preserves outgoing original-error flow through generic Pure and legal typed [error] Proc statement propagation; missing ERROR permission refuses. Auxiliary lexical loop controls retain exact original expression types/producer flows across nested and deferred control boundaries, with unchanged cold query counters and incompatible break/written-type refusals. Actual compiler regressions and authored fixture corrections are distinguished in JSON.

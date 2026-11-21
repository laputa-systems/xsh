##! Package types.

## A package catalog.
export type Catalog = {name: Str}

## A build target used by the entry.
export type Target = {triple: Str}

## A file kind used through its variants.
export enum Kind { Payload, Meta }

## A shape no module uses.
export type UnusedShape = {size: Int}

## A receipt no module uses.
export type UnusedReceipt = {digest: Str}

## A plan no module uses.
export type UnusedPlan = {steps: List[Str]}

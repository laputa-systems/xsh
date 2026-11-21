##! Plan shapes.

## Whether a plan action builds locally.
export pure plan_action_is_build(action: Str) -> Bool {
  return action == "build"
}

## The label for a plan action.
export pure plan_action_label(action: Str) -> Str {
  return action
}

## The plan format.
export const PLAN_FORMAT = "v1"

## One plan step.
export type PlanStep = {action: Str}

## One plan receipt.
export type PlanReceipt = {action: Str}

## An export only this module reads.
export pure own_only() -> Int {
  return 7
}

pure private_uses_own_only() -> Int {
  return own_only()
}

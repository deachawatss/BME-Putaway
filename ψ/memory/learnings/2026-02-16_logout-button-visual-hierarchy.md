# Learning: Logout Button Visual Hierarchy in Industrial Apps

**Date**: 2026-02-16
**Source**: BME-Putaway dashboard removal session
**Context**: Warehouse putaway bin transfer system

## The Pattern

In warehouse/industrial applications, the logout/sign out button should use **high-contrast destructive styling** (red background + white text), not subtle ghost buttons or secondary styles.

## Why This Matters

1. **Context of use**: Warehouse workers operate in time-sensitive, sometimes fatiguing conditions
2. **Action clarity**: Logout is a terminal action — it should be immediately distinguishable from workflow actions
3. **Safety**: Users need to know they're exiting the application, not triggering a workflow step

## The Anti-Pattern

Avoid these styles for logout:
- Ghost/transparent buttons (too subtle)
- Grey backgrounds (looks disabled)
- Matching primary workflow buttons (creates confusion)
- Amber/warning colors (reserves that semantic for business logic warnings)

## The Solution

```html
<button
  class="tw-bg-red-600 hover:tw-bg-red-700 tw-text-white
         tw-px-6 tw-py-2 tw-rounded-lg tw-text-sm tw-font-semibold
         tw-transition-all tw-duration-200 hover:tw-scale-105">
  Sign Out
</button>
```

## Application

Apply this pattern to:
- Logout/Sign Out buttons
- Session termination actions
- Exit application flows

Do NOT apply to:
- Cancel buttons in forms (use neutral grey)
- Back/return navigation (use outline or text link)
- Secondary dismiss actions

## Related Patterns

- Primary actions: Brand color (amber/brown for NWFTH)
- Secondary actions: Outlined or grey
- Destructive actions: Red background
- Disabled actions: Greyed out with reduced opacity

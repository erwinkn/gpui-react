# React wrappers for native GPUI controls

`@gpuix/bridge-controls` supplies typed React wrappers. The application composition
must register the matching [`gpui-react-controls`](../../crates/gpui-react-controls)
Rust crate. React and `@gpuix/bridge` are peer dependencies.

```tsx
import { createRef } from "react"
import { Input, type InputRef } from "@gpuix/bridge-controls"

const input = createRef<InputRef>()
root.render(<Input
  ref={input}
  initialValue="Hello"
  label="Message"
  initialMultiline
  maxRows={6}
  style={{ fontSize: 16, padding: 8, background: "#202020", color: "white" }}
  onEvent={event => {
    if (event.type === "change") saveDraft(event.snapshot.value)
  }}
/>)

// After mounting, native queries and commands are asynchronous.
const snapshot = await input.current!.query(null)
await input.current!.command({
  type: "replace", value: "Replacement", expectedRevision: snapshot.revision,
})
```

Native state owns the live text. Props do not echo it back on each React render.
`replace` rejects if typing, selection, or composition has advanced since the
specified revision. It also rejects during active composition, even if the
revision matches. Catch that rejection and choose whether the application still
needs the replacement. There is no automatic retry or forced overwrite.

| Input prop | Behavior |
| --- | --- |
| `initialValue` | Initial text, default empty. Changes after construction do not replace text. |
| `initialMultiline` | Initial editing mode, default false. Remount to change it. Single-line edits replace line breaks with spaces. |
| `placeholder` | Text shown when the buffer is empty; default empty. |
| `label` | Accessible name; default empty. The accessible value always follows native text. |
| `readOnly` | Blocks user edits, paste, undo, and IME replacement. Explicit application replacement remains available. |
| `minRows`, `maxRows` | Multiline visible-row bounds, default 1 and 10. Minimum is at least one; maximum is at least minimum. Single-line inputs use one row. |
| `submitOnEnter` | For multiline input, Enter emits submit and Shift-Enter inserts a newline. Otherwise Enter inserts a newline. Single-line Enter always emits submit. |
| `captureKeys` | Unmodified key names intercepted natively while an application menu is open, for example `['up', 'down', 'escape']`. Disabled during IME composition. |
| `style` | Native style fields below. |
| `caretColor`, `selectionColor` | CSS colors. Defaults are white and `#7c86ff59`. |
| `onEvent`, `ref` | Standard bridge event handler and asynchronous native ref. Input does not accept child views. |

Commands are `focus`, `blur`, `replace`, and `select`. `replace` carries `value`
and `expectedRevision`. A successful changed replacement is one undo step;
a same-value replacement preserves selection and history. `select` carries
`selection: {start, end, reversed?}` and `expectedRevision`. Offsets use UTF-16.
Out-of-range positions and positions inside a surrogate pair reject. A stale
command leaves native state unchanged. Focus and blur act through GPUI and do
not activate or raise an OS window.

`query(null)` returns `{value, revision, selection, composing, painted}`.
`painted` is null before the first paint, or `{x, y, width, height, revision, frame}` in
logical window pixels. It describes the text content box from the last paint.
It can be older than the current state. A query does not force a frame.

Events are:

- `{type: 'change', snapshot}` after text edits, including composition updates,
  explicit replacement, undo, and redo.
- `{type: 'selection', revision, selection, composing}` after selection changes
  and composition termination without replacement.
- `{type: 'submit', value, revision}` on submission.
- `{type: 'key', key}` for a declared captured key.

Events reach JavaScript asynchronously. Native editing does not wait for them.
The current API does not emit general focus, blur, or uncaptured key events.

`Style` supports these fields; unknown fields and invalid colors reject:

| Fields | Values |
| --- | --- |
| `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight` | Logical pixels, `"100%"`, or `"auto"`. |
| `direction` | `row`, `column`, `rowReverse`, `columnReverse`. |
| `align` | `start`, `center`, `end`, `stretch`. |
| `grow`, `shrink` | GPUI flex factors. |
| `gap`, `padding`, `paddingX`, `paddingY` | Logical pixels. Axis padding overrides general padding. |
| `background`, `color`, `borderColor` | CSS colors, parsed once at prop admission. |
| `fontSize`, `lineHeight`, `borderWidth`, `radius` | Logical pixels. Text styles inherit from GPUI when omitted. |
| `opacity` | GPUI opacity. |
| `hover`, `active`, `focus` | Style objects applied by GPUI's interaction states. Nested interaction refinements are not applied. |

The input clips its content and scrolls internally as needed. A wheel that moves
it consumes native scrolling but still reaches ancestor callbacks. At a boundary,
a parent can scroll. Native tests cover both cases. Layout, selection, caret
movement, drag autoscroll, clipboard operations, and undo stay in Rust.

## Containers and text

`Container` renders a normal GPUI div. It owns its child handles, focus handle,
and scroll handle. Props are `style`, `scroll` (`none`, `x`, `y`, or `both`),
`focusable` (default false), `label`, `scrollGroup`, and `blockMouse` (default
false). Layout defaults to a flex column. Text styles inherit through GPUI.
Children are ordinary native views. Keyed React moves retain their identities.

Normal GPUI hit testing allows a parent to receive clicks through text and
nested layout containers. `blockMouse` explicitly isolates the mouse region
through GPUI's `block_mouse_except_scroll`; wheel observers still receive events.
There is no synchronous JavaScript event cancellation.

Container events are `click` with logical window coordinates `{x, y}`, and
`wheel` with `{x, y, dx, dy, offset}`. Wheel deltas use GPUI's signed pixel
convention. `offset` uses positive scroll distances from the top and left. It is
read after native wheel handling. A boundary event can scroll an ancestor;
consuming an event does not suppress its callbacks. Two-axis scrolling applies
both deltas. An x-only viewport does not turn vertical wheel motion into x motion.

Commands are `focus`, `blur`, and `scrollTo: {x, y}`. Focus requires `focusable`.
Scroll coordinates are positive distances from the origin; GPUI clamps them at
layout. `query(null)` returns `{painted, revision, offset, childCount, focused}`.
`painted.bounds` is the outer layout box, including padding and border.

For locked horizontal panes, give x-only containers the same nonempty
`scrollGroup` string. They then use one GPUI `ScrollHandle` within that window.
They must have equal viewport and content widths. Different groups remain
independent; removing the group creates a new independent handle at the origin.
The group is ignored on other scroll modes. No offset is copied through React.

`Text` takes `text: string` and optional `style`; it is a leaf. Its asynchronous
`query(null)` returns `{text, revision, painted}`. Here `text` is the current
native value, while `painted` describes its last paint. This initial text control
does not yet provide document selection or search. Those services remain under
implementation. A primitive string child also creates a text leaf. For a complete
line with interpolation, prefer ``<Text text={`Hello ${name}!`} />``; separate
native text children remain separate layout items.

## Lists

`List` uses GPUI's variable-height `ListState`. Its direct native children are
rows. Wrap a row's content in a `Container` when it contains multiple host nodes,
conditional content, or a Suspense boundary. Its props are:

| Prop | Behavior |
| --- | --- |
| `style` | Outer list layout and inherited text styles. Supply a bounded height. |
| `itemCount` | Optional logical count. If omitted, the native child count is the count. |
| `windowStart` | Logical index of the first supplied child, default zero. Used with `itemCount`. |
| `estimatedItemHeight` | Height hint for unmeasured or missing rows. Defaults to the window line height when props are applied; minimum one logical pixel. |
| `overdraw` | Extra measurement distance around the viewport, default zero; clamped to nonnegative pixels. |
| `alignment` | `top` (default) or `bottom`. |
| `followTail` | Initially follows the end when true. Native upward scrolling pauses following. Returning to the end resumes it. |

Large logical lists do not need large React trees. For example, supply sixty
children starting at 49,998 with `itemCount={100_000}`. GPUI keeps its native
height index for the logical count and builds visible row elements. The bridge
retains only the supplied native views. The height index still uses memory
proportional to the logical row count.

`needRows` reports `{range: {start, end}}` when layout needs absent rows. `end` is
exclusive. Requests can include overdraw and speculative measurements. Missing
rows reserve the estimated height. Repeated requests are suppressed until the
range or native request changes. `scroll` reports `{range, followingTail}` after
native scrolling. The application owns loading, request cancellation, and stale
response rejection; the renderer cannot display data it has not received.

Commands are:

- `{type: 'scrollTo', index, offset?}` sets a logical anchor. Offset defaults to
  zero. A negative offset places the viewport above the chosen row; GPUI resolves
  it against real row heights during layout. The count itself names the end.
- `{type: 'end'}` scrolls to the end and resumes `followTail` when enabled.
- `{type: 'remeasure', start, end}` invalidates native row height measurements.
  Use this when native code changes offscreen content outside React transactions.

React prop and descendant-structure updates invalidate affected row measurements
before later commands in the transaction. A changed row window and a scroll
command from a synchronous layout effect therefore reach the same native layout.
For a focus target not yet rendered, scroll its row into the viewport before
sending its focus command. An already rendered focused row uses GPUI's native
focus tracking; focus registration does not erase height estimates.

A prepend at the top shows the new first rows. A reader below the top retains
their row and pixel offset. Keyed reorders also retain an existing reader anchor.
A removed anchor uses GPUI's splice behavior. In a windowed list, the application
owns logical index changes and should commit an explicit anchor with its new
window when it inserts earlier data.

`query(null)` returns `itemCount`, `supplied`, `anchor: {index, offset}`,
`followingTail`, `revision`, `painted`, `paintedRows`, and `maxScrollY`.
`paintedRows` records row paint callbacks; `maxScrollY` uses native measured
heights and estimates. Both can lag current props until layout. Changes to
alignment, overdraw, or the height estimate rebuild the native list index while
preserving its anchor where possible. Current count changes also revisit the
height index; performance validation of this path remains open.

## Frame metadata

Painted measurements include `frame: {root, frame, commit, viewportWidth,
viewportHeight, scaleFactor}` when rendered below a bridge `Host`. `root` is an
opaque string identifying that native host within its application session. `frame` identifies its draw;
`commit` is the latest native transaction incorporated into that draw. Viewport
sizes use logical pixels. A native component outside a bridge host has a null
frame tag.

The component's own `revision` is separate. A parent's padding or font can change
a leaf's layout without changing that leaf's props. Use the frame tag to identify
the draw, rather than assuming matching local revisions prove fresh layout.
These tags identify GPUI paint work, not physical OS presentation. Read-only
transactions do not force another draw.

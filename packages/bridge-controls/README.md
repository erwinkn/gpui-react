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
`painted` is null before the first paint, or `{x, y, width, height, revision}` in
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

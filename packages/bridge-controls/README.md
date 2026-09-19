# React wrappers for native GPUI controls

`@gpuix/bridge-controls` supplies typed React wrappers. The default
`@gpuix/bridge-runtime` package registers all controls below. A custom application
composition must register the matching
[`gpui-react-controls`](https://github.com/erwinkn/gpuix/tree/bridge/minimal-react-gpui/crates/gpui-react-controls)
Rust crate. React and `@gpuix/bridge` are peer dependencies. Install matching
versions of the bridge packages.

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

`Text` accepts `text: string` or string/number children, plus optional `style`.
For example, `<Text>Hello {name}!</Text>` joins interpolation into one native
text value. Supplying both forms, or a React element as a text child, throws.
Use native GPUI text runs for multiple styles inside one logical text. Primitive
strings outside `Text` remain separate native layout items.

`textKey` gives text a stable logical identity within a `Document`. Supply it
when virtualized rows can unmount and remount. Without it, the native entity
provides an identity for its own lifetime. `selectable` and `searchable` both
default to true. Set either to false independently. `matchIndexOffset` is an
optional absolute match base for this text in a virtualized source.
`query(null)` returns `{text, revision, painted}`. The value is current native
text; the geometry is from its last paint.

## Document text services

Wrap selectable content in `Document`. It is an ordinary native GPUI view with
`children`, `style`, `search`, `selectionColor`, `onEvent`, and `ref`.
Selection color defaults to `#3875d799`. Nested documents have separate
selection, focus, and search state. Native components can join the same scope
through the Rust `document_text` helper.

```tsx
import { createRef } from "react"
import { Document, Text, type DocumentRef } from "@gpuix/bridge-controls"

const document = createRef<DocumentRef>()
root.render(<Document ref={document} search={{ query: "reader", activeIndex: 0 }}>
  <Text textKey="greeting">Hello {name}! Welcome, reader.</Text>
  <Text textKey="footer" selectable={false}>Read-only chrome</Text>
</Document>)

// This query does not force a draw. Check that content has painted first.
const snapshot = await document.current!.query(null)
if (snapshot.text.some(text => text.key === "greeting")) {
  await document.current!.command({
    type: "select", expectedContentRevision: snapshot.contentRevision,
    start: { key: "greeting", offset: 0 }, end: { key: "greeting", offset: 5 },
  })
}
```

Dragging, double-click word selection, triple-click paragraph selection,
clipboard keys, and drag autoscroll run natively. A simple click can still reach
a clickable parent. Copy and Select All keyboard handling belongs to the
focused document; an input inside it keeps its own editor shortcuts. Autoscroll
uses native container/list handles and can continue after the anchor row leaves
the viewport. It needs overlapping painted content between scroll steps.

Commands are `clear`, `copy`, `selectAll`, and `select`. `select` takes start and
end `{key, offset}` endpoints plus `expectedContentRevision`. Offsets use UTF-16.
Invalid offsets, split surrogate pairs, stale content revisions, and endpoints
absent from the paint registry reject. `selectAll` and `select` reject before
the first paint. They select registered logical texts, not unloaded rows.
For a full export, read the application data model.

The selection retains shared source text after rows unmount. Copy returns that
snapshot until selection changes or clears. It is not a live range remapped
through document edits. A selection wash appears only where the selected bytes
still match the current logical text at that range. Copy inserts one newline
between selected logical texts.

`search` accepts:

| Field | Behavior |
| --- | --- |
| `query` | Query string. Empty queries produce no matches. |
| `regex` | Treat the query as a Rust regex when true; default false. Invalid regexes reject the transaction during prop validation. Zero-length matches are excluded. |
| `caseSensitive` | Default false. |
| `wholeWord` | Add Unicode word boundaries around the query; default false. |
| `activeIndex` | Optional zero-based match index for the active color. |
| `matchIndexOffset` | Default zero. Match count before the registered content window. |
| `color`, `activeColor` | CSS colors, default `#ffd54d66` and `#ff9900aa`. |

Search matches each logical text separately; it does not join the document or
match across paragraph boundaries. Matching uses painted text from native and
React components in paint order. Nonselectable text stays searchable unless
`searchable={false}`. Native helpers can contribute styled runs as one logical
text, so style boundaries do not split a match.

For a virtual list, give each supplied text its absolute `matchIndexOffset`
when the application knows that prefix. It overrides the document offset for
that text. Native scrolling can then preserve global active match numbering
without a React update. Counts cover registered text only. The application
owns the full-source count and prefixes for a query. Do not use row indices as
match prefixes unless each row has exactly one match.

Native caches reuse matching ranges when only colors, the active index,
geometry, or match offsets change. Prop admission still validates the regex.
The cache retains only text present in the latest paint, not earlier windows.

`query(null)` returns:

- `text`: registered entries with `{key, text, bounds, selectable, searchable}`.
- `contentRevision`: version of registered text, order, and text options.
- `selection`, `selectionRevision`: current selected text and its version.
- `paintedSelectionRevision`, `ranges`: selection version and geometry from the
  last paint. Each range is `{key, start, end, rects}` with UTF-16 offsets.
- `highlights`: the same range format plus global `index` and `active`.
- `matchCount`, `matchIndexOffset`, `query`: the registered count, scope offset,
  and query settings from the last paint.
- `frame`: last paint's bridge frame tag, or null outside a bridge host.

Bounds and rectangles use logical window pixels and the paint clip. Current
selection can be newer than painted selection geometry. A query never performs
layout. Geometry-only changes advance the frame while content revision stays
unchanged. Content revision can also change as virtualization changes the
registered window.

Events are `{type: 'selection', revision, hasSelection}` and
`{type: 'search', query, frame, contentRevision, count, indexOffset}`. Selection events carry
no joined text; query or copy when needed. Search events follow the complete
paint and distinguish query changes even when the match count is equal.
The event includes its query settings and paint frame, so delayed results retain
their source. Active-index and color-only changes do not emit new search results. A listener
added later receives future events; use a query for the current snapshot.

GPUI cached-view replay skips paint callbacks. Keep document content uncached
until the integration has a tested way to replay its text metadata. The current
controls do not use cached-view replay.

Native components can use ordinary GPUI deferred drawing inside a document.
Their `document_text` content remains selectable and searchable in the nearest
document. Nested documents keep separate registries. Content revisions and
search counts include all deferred paint before the native draw returns; events
still reach JavaScript asynchronously. A query does not force another draw.


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
before later commands in the transaction. The native layout pass also detects
inherited typography changes and invalidates measured heights before resolving
anchors. A changed row window and a scroll
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
preserving its anchor where possible. Count changes splice hints into the affected native index range and preserve
existing measurements. See the [native mutation measurements](../../docs/bridge-list-performance.md).

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

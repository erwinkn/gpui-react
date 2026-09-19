# React on GPUI: concrete design examples

These examples compare proposed designs. Code for transactions, revisions, and measurement is pseudocode, not an API we have implemented. The letters match the reviews.

| Option | Native description model | Layout |
| --- | --- | --- |
| A. Typed transactions | One mutable Rust tree on UI; React keeps JS host instances | GPUI on UI |
| C. Shared revisions | Immutable Rust descriptions built on the worker and shared with UI | GPUI on UI |
| B. Prepared layout | Shared revisions plus candidate layout results | Pure native layout on a worker, then checked adoption on UI |

All three keep the active editor, IME, focus, scroll position, and animation state native. All three can call pure Rust services from the worker. A has no worker Rust tree; it does not require every worker operation to be JavaScript.

## 1. Change a counter from 0 to 1

The application is the same under every option:

```tsx
function Counter() {
  const [count, setCount] = useState(0);
  return (
    <div onClick={() => setCount(n => n + 1)}>
      <text>Count: {count}</text>
    </div>
  );
}
```

Suppose the button has host id 20 and the changing text leaf has id 21. The sidebar and editor are siblings of the button.

### A: send an operation

React commits. The worker seals this transaction after layout effects:

```rust
Transaction {
    sequence: 18,
    ops: [SetText { id: 21, value: "1" }],
    commands: [],
}
```

At a safe point, UI changes node 21 in its mutable tree. GPUI then builds the required elements, computes layout, and paints. The worker does not need a native copy of node 21 to send this operation.

### C: publish a new description

React builds a new native description for node 21 and new descriptions for affected ancestors. The unchanged sidebar and editor descriptions remain shared:

```text
Revision 17                   Revision 18
root17                        root18             new
├─ sidebar ───────────────────├─ same sidebar     shared
├─ button17                   ├─ button18         new
│  └─ text "0"                │  └─ text "1"      new
└─ editor ────────────────────└─ same editor      shared
```

The UI accepts revision 18 and compares it with the revision it has mounted. It updates the affected native instances. GPUI still computes layout on UI.

This is a new logical version of the description tree. It is not a full copy of every node. Nor is it free: affected ancestor nodes and child sequences still need work. React's persistence implementation can also enumerate siblings.

### B: publish descriptions and prepared layout

The worker prepares revision 18 and computes its geometry against captured inputs, such as the current viewport and font version. UI accepts that geometry only if its dependencies are still valid.

For a tiny counter, A may have less overhead. C becomes useful when other tasks need stable description revisions. B becomes useful if layout work is substantial or React needs candidate measurements. This example alone gives no reason to expect C or B to be faster.

## 2. Several updates arrive before UI can consume them

Suppose a streaming message changes through three committed states:

```text
"Hello" → "Hello there" → "Hello there!"
```

### A: consume ordered changes

```text
SetText(21, "Hello")
SetText(21, "Hello there")
SetText(21, "Hello there!")
```

UI applies the transactions in order. It can draw only after all three have applied, so this does not require three visible frames. It still pays the application cost for each required operation.

A can optimize repeated text assignments if the protocol proves that no intervening command or observation needs an intermediate value. That is additional coalescing logic, not an inherent prohibition.

### C: select a complete revision

```text
mounted revision: R17
pending revisions: R18, R19, R20

UI can compare R17 directly with R20.
```

If R18 and R19 contain only replaceable visual state, UI need not mount them. Their descriptions can be released once nothing else references them. Complete revisions make this choice direct.

B has the same opportunity, but should also cancel preparation for obsolete candidates. A layout result for R18 is no longer useful if UI will adopt R20.

### Add a command and the rules change

```text
R18: create input A
R18 layout effect: focus(A)
R19: remove input A
```

If focus is an ordered command, C cannot discard R18 and then pretend it executed focus. It must preserve the required sequence: create, focus, remove. That does not require presenting every intermediate scene.

Alternatively, the API can define focus as best effort and return a rejected result when A is gone. That is a different contract. The tree representation does not choose it for us.

The practical question is how much replaceable visual work occurs between commands. C's ability to skip revisions is valuable only when there is useful work to skip.

## 3. Position a tooltip using its measured height

Imagine a tooltip near the top of a window. React wants it above the target if it fits, otherwise below.

```tsx
// Desired measurement behavior. measure() is an illustrative API.
useLayoutEffect(() => {
  const { height } = tooltip.current.measure();
  setSide(height <= roomAbove ? "above" : "below");
}, [message, roomAbove]);
```

### A and C with UI layout

When this effect runs, UI may not have laid out the new tooltip. A synchronous snapshot can describe an older frame, or have no bounds for a newly created tooltip.

An asynchronous measurement can wait for native layout. React can then send a correction, but that does not make the original layout effect synchronous. The first placement may already have appeared.

Both designs can avoid this problem for the tooltip by letting a native anchored component choose the side during layout. React declares the relationship; GPUI has the target bounds and tooltip size at the time it needs them.

Shared descriptions alone do not improve this example's measurement semantics. C still performs final layout on UI.

### B with candidate layout

The intended sequence is:

```text
worker: construct tooltip candidate
worker: compute candidate height = 72
worker: layout effect reads 72 and chooses below
worker: prepare corrected candidate
UI:     validate dependencies and adopt the result
```

UI can continue to display and operate its previous revision while this happens. It need not display the first tooltip candidate.

Now resize the window during preparation. The tooltip wraps differently and becomes 96 pixels high. The old result is invalid. B must reject or recompute it, and determine whether React's placement decision must also run again. A normal layout effect does not automatically rerun because a native dependency changed.

That is B's main architectural cost. It needs a coherent candidate-layout contract, native-component measurement inputs, checked adoption, and rules for stale JS decisions. Current GPUI does not provide that general contract.

## 4. Type while React is delayed

The editor starts with an empty value. The user types two characters while the worker is busy:

```text
native edit 41: "a"
native edit 42: "ab"

React later handles edit 41 and sends value "a".
```

The editor must retain "ab". It cannot apply the late value just because it arrived in a newer React commit.

All three designs need a rule such as:

```rust
ValueAcknowledgement {
    text: "a",
    observed_edit: 41,
}

// UI editor is already at edit 42.
// This acknowledgement must not replace its live text.
```

An intentional replacement is a separate operation. For example, React may normalize "ab" to "AB" against edit 42. UI must check that version before applying the replacement, and check it again if composition delayed the operation. An unconditional reset needs an explicit contract.

A applies the acknowledgement from a transaction. C finds it in a new description revision. B also needs to invalidate any prepared text geometry that depended on the older editor state. The native editor remains the owner of immediate typing and IME in every case.

This is why smooth native input does not depend on keeping a worker Rust tree. Correct ownership and stale-update rules provide that property.

## 5. Scroll a chat while the worker parses new content

The current chat contains a long code block. The user scrolls while another message arrives.

Both A and C can prepare syntax spans and document structure off UI using the new message's props. C does not have an exclusive claim to background work.

If a background task needs a consistent view of many nodes, the difference becomes clearer. C can give it a retained immutable revision. A must give it suitable JS data, a snapshot, or a query result. C makes those concurrent native readers easier; A avoids paying for them when no task needs them.

The scroll frame is a separate cost. It can still spend time rebuilding native elements, measuring visible text, and constructing the scene under either model. Publishing an immutable root does not cache those operations.

GPUI subtree caching needs a separate design for constraints, invalidation, text selection, highlight order, bounds, and deferred overlays. Both A and C can use it if it works. B can reduce some layout work, but still has native interaction and scene work to perform.

Also, a live scroller cannot show content the application has not supplied. A React-windowed list needs a placeholder or retained data when the worker is blocked and the user reaches an unavailable row. A native document can render content it already owns without another React commit.

## What the examples distinguish

| Question | Choice it tests |
| --- | --- |
| Is applying a small typed transaction cheaper than building and mounting a shared revision? | A versus C |
| Do we frequently skip substantial obsolete visual work or run concurrent native readers? | The main reasons to choose C |
| Do components need synchronous candidate geometry for React decisions? | The main reason to examine B |
| Can visible native content remain responsive during JS stalls? | Native ownership, required under all options |
| Does each scroll frame rebuild too much native content? | Native rendering and caching, separate from A versus C |

The strongest reason to choose A is that its ordered changes may already be small enough. The strongest reason to choose C is useful work on stable revisions and direct publication of selected states. B adds stronger candidate-measurement behavior, at the cost of a new layout and adoption contract.

# Third-party notices

## Ported source

The text input component in `crates/gpui-react/src/input.rs` and document selection in `crates/gpui-react/src/document/selection.rs` adapt code from **[Comet](https://github.com/zeronsh/comet)** (MIT, Copyright (c) 2026 Wing).

Input caret blinking, double-click selection, drag autoscroll, and undo behavior follow Comet's composer, reviewed at commit `b3fa51872f70c8f973c241b659cf0c166766f4f5`.

Document selection adapts Comet's selection logic, including the soft-wrap geometry fix at `f6911c311dc654734d31bc3097a84fb73659939f` and the virtualized drag correction at `3536a3702ca405fec1321e95f54e280240c5d38f`.

### Comet MIT license

Copyright (c) 2026 Wing

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

## Other dependencies

| Component | License | Source |
| --- | --- | --- |
| GPUI | Apache-2.0 | https://github.com/zed-industries/zed |
| GPUiX | Apache-2.0 | https://github.com/remorses/gpuix |

GPUiX (Apache-2.0, https://github.com/remorses/gpuix) is the origin of the Comet selection port and the inspiration for this project.

gpui-react is licensed under the terms in `LICENSE`.

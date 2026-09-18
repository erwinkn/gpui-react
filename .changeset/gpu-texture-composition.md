---
"@gpuix/native": minor
---

Add GPUI texture composition through the renderer's shared device and queue.
Support premultiplied RGBA textures, float highlights, inherited opacity,
clipping, and corner radii. Keep effect shaders in external native extensions.

Remove the Glimm-specific payload from core background, quad, and path layouts.
The consumer owns the preserved Metal/WGSL formulas and license.

Fix wgpu surface parameter reuse so each draw retains its own bounds and opacity.
The upstream external compositor proposal remains closed pending API discussion:
https://github.com/zed-industries/zed/pull/60573.

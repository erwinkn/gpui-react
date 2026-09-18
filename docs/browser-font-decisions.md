# Browser font binding decisions

- Use the GPUI application text system before a graphics window exists. The alternative was to delay font registration until after mount, which could change the first layout. Confidence: high. Calls still require the renderer to be initialized.
- Use a temporary WindowTextSystem for measurement, as the native worker host does. The alternative was to require a window. Confidence: high. Hosts should cache repeated measurements.
- Exclude the N-API syntax-token annotation on WebAssembly. The alternative was to add a desktop binding dependency to the web target. Confidence: high. The native API remains unchanged.

Validation: the combined Pierre integration compiled for macOS arm64 and WebAssembly. Its standalone page registered fonts, measured labels, and rendered through browser WebGPU. Desktop editor input and pixel checks passed. No native host lifecycle code changed in this commit.

I stand behind these choices for the tested targets. The user authorized this fork and release work; this commit is for integration with the runtime thread.

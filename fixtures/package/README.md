# Installed bridge package check

`bun scripts/test-bridge-packages.ts /path/to/archives` installs the three
archives in a temporary directory outside the workspace. It checks package
hashes, Node and Bun loaders, ESM and CommonJS types, the browser error,
and this hidden native application. It then compiles the application, removes
the installation and source, and runs the moved executable from another path.

The application uses all five standard controls. It checks the first native
draw, layout-effect scrolling, document search and Unicode selection, native
input state across React updates, stale-command rejection, event delivery,
and unmount. A separate process verifies that test components are absent.
Queries do not force later draws. Platform input and GPU pixel tests stay in
the native control suite and the counter interaction fixture.

Add `--published` to install the manifest's release URLs instead of local
archives. Published manifests must identify clean source.

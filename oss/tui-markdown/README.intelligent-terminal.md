# tui-markdown integration

This directory contains the tui-markdown 0.3.11 crate from crates.io.
Its source is the `tui-markdown` subdirectory of upstream commit
`493efd16d396b5c927832a7f2265d9ec881c7961` in
<https://github.com/joshka/tui-markdown>.
The original crate archive has SHA-256
`6cbd479859da89c70663630e3e455895038d337ccfeed132f2108fa89647c45c`.
The upstream dual licenses are retained in `LICENSE-MIT` and `LICENSE-APACHE`.
Archive metadata, the upstream lockfile and changelog are omitted.

Two changes in `src/renderer/code.rs` are carried locally:

- Include the syntax theme's background with its foreground colors, so highlighted
  text remains readable when the surrounding terminal profile has a light background.
- Insert highlighted lines through the existing prefix/style-aware writer, so
  quotes and list indentation behave like ordinary code blocks.

All other crate source files are unchanged. WTA keeps its exact version constraint
and uses this directory as a path dependency. Its runtime dependencies remain in
the generated WTA notices; this source copy has its own Component Governance
manifest and attribution in the root notice file.

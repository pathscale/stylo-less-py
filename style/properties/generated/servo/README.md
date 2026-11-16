# Pre-generated Servo properties

These files are the deterministic output of Stylo's Servo property generator, run on the
property data and templates in this repository (`style/properties/longhands.toml`,
`shorthands.toml` and the `*.mako.rs` templates). They include this repository's own changes to
that data, so they are not identical to the output of any upstream commit:

- `properties.rs`
- `css-properties.html`
- `css-properties.json`

They are committed so normal Servo builds need only Rust and Cargo. Do not edit them by hand.
When changing Stylo's property definitions or templates, regenerate all three files together
and verify their diff before updating this directory. To regenerate, run
`python3 properties/build.py servo style-crate` from the `style` directory (the same command
the non-Servo build path runs) and copy the three outputs here.

# Porting `properties.rs`

Two of the three artifacts `build.py` emits are generated here and proven by byte
equality. The third, `properties.rs`, is not, and is committed under
`style/properties/generated/servo` as the generator's existing output.

This is what finishing it involves. It is written down because the remaining step is
not the same size as the two that are done, and nothing else in the tree says so.

## Where it stands

| artifact | lines | template | generated in Rust |
|---|---:|---|---|
| `css-properties.json` | 1,341 | (built from the model directly) | yes, byte-equal test |
| `css-properties.html` | 1,810 | `properties.html.mako` (31 lines) | yes, byte-equal test |
| `properties.rs` | 123,135 | `properties.mako.rs` (3,099 lines) | **no** |

The two ported artifacts are inventories: a sorted list of properties rendered as data.
`properties.rs` is a Rust program. That difference, not the line count, is the work.

## Templates in scope

Servo builds pull in, by `<%include>` and `<%namespace>`:

- `properties.mako.rs` — 3,099 lines, the entry template
- `helpers.mako.rs` — 784 lines, the per-property codegen helpers
- `helpers/animated_properties.mako.rs` — 762 lines

`gecko.mako.rs` (1,553 lines) is out of scope: the Gecko path keeps its Python generator
and is not built here.

So roughly 4,600 lines of template logic, not 3,099.

## What the model already gives you

`stylo_property_model` loads the TOML into typed data and is not the gap:

`PropertyDatabase`, `Longhand`, `Shorthand`, `Alias`, `Descriptor`, `Keyword`, `Vector`,
and the supporting enums (`Engine`, `Affects`, `AnimationType`, `EnabledIn`, `RuleType`,
`RestyleDamage`, `AllowQuirks`).

The JSON and HTML ports consume exactly this. The missing piece is the rendering layer
for a much larger and more conditional output.

## Known blocker, already found

`stylo_property_model/src/lib.rs` carries an `UNVERIFIED` note that has to be resolved
before byte equality is achievable:

> `data.py` sorts style structs on `StyleStruct.name`, which is CamelCase, while this
> sorts on the TOML's lowercase `struct` value. The two agree for every name in the file
> today, since case is uniform and no name differs only by separator, but this ordering
> is observable in the generated `all` shorthand.

Two names that differ only by case or separator would reorder the `all` shorthand and
break byte equality in a way that looks like a codegen bug rather than a sort key
mismatch. Fix the sort key first; do not discover this from a 123k-line diff.

Expect more of these. The two ported artifacts found none because sorted inventories have
almost no ordering surface; `properties.rs` has ordering embedded throughout.

## The standard

Byte equality against the committed artifact, as with the other two. From the crate docs:
a diff of exactly zero is the only evidence that swapping the generator changes nothing
that ships. Anything less is a rewrite that happens to compile.

Practically, this means the port cannot land incrementally the way the first two did:
`properties.rs` is one file, so it is either byte-identical or it is not. Splitting the
work by template section and diffing rendered fragments against the committed file is the
only way to get intermediate signal.

## Suggested order

1. Fix the style-struct sort key and confirm the `all` shorthand ordering against the
   committed file.
2. Port `helpers.mako.rs` first. It is the leaf: `properties.mako.rs` calls into it, so a
   correct helper layer makes the entry template mostly control flow.
3. Port `properties.mako.rs` section by section, diffing each rendered region against the
   corresponding region of the committed `properties.rs`.
4. Port `helpers/animated_properties.mako.rs`.
5. Add the byte-equality test alongside the existing two.
6. **Delete the committed `properties.rs`** and have `build.rs` invoke the generator
   instead of `copy_prebuilt_servo_properties`. This is the point of the exercise: the
   tools are committed, the output is not.

## Until then

The committed `properties.rs` is not a permanent artifact. It is doing two jobs: keeping
the Servo build free of Python, and serving as the fixture the port is checked against.
Both end at step 6.

Note that `build.rs` panics when it is absent, so it cannot simply be deleted early:

```
thread 'main' panicked at style/build.rs:91:13:
failed to copy .../generated/servo/properties.rs to .../out/properties.rs
```

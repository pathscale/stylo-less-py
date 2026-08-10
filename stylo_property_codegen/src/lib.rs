//! Generates the artifacts `build.py` emits, without `build.py`.
//!
//! `build.py` produces three files: `properties.rs`, `css-properties.json` and
//! `css-properties.html`. This crate replaces them one at a time, and the test
//! for each is byte equality against the committed output in
//! `style/properties/generated/servo`. Anything less than byte equality is not
//! a port, it is a rewrite that happens to compile.
//!
//! # Why byte equality and not "looks right"
//!
//! The committed artifacts are what Servo builds today, so a diff of exactly
//! zero is the only evidence that swapping the generator changes nothing. It
//! also gives the drift check for free: once a file is generated here, the same
//! comparison catches the committed copy falling out of step with the TOML it
//! came from, which nothing checks today.

use std::fmt::Write as _;

use stylo_property_model::{EnabledIn, Engine, PropertyDatabase};

/// The property inventory published for rustdoc, as `build.py` writes it.
///
/// Shape, from `build.py`: two objects, `longhands` and `shorthands`, each
/// mapping a property name to `{"pref": servo_pref}`. A property appears only
/// if it is enabled in content, and each of its aliases appears as its own
/// entry carrying the same preference, because an `Alias` inherits `servo_pref`
/// from the property it aliases.
pub fn css_properties_json(database: &PropertyDatabase) -> String {
    let longhands = database
        .longhands_for(Engine::Servo)
        .filter(|property| property.enabled_in() == EnabledIn::Content)
        .flat_map(|property| {
            std::iter::once((property.name.clone(), property.servo_pref.clone())).chain(
                property
                    .aliases()
                    .into_iter()
                    .map(move |alias| (alias.name, property.servo_pref.clone())),
            )
        });
    let shorthands = database
        .shorthands_for(Engine::Servo)
        .filter(|property| property.enabled_in() == EnabledIn::Content)
        .flat_map(|property| {
            std::iter::once((property.name.clone(), property.servo_pref.clone())).chain(
                property
                    .aliases()
                    .into_iter()
                    .map(move |alias| (alias.name, property.servo_pref.clone())),
            )
        })
        // `all` is not in shorthands.toml: data.py synthesises it, with no
        // preference and no aliases. Omitting it here cost a shorthand.
        .chain(std::iter::once(("all".to_owned(), None)));

    let mut out = String::from("{\n");
    // `json.dumps(..., indent=4, sort_keys=True)`. Both groups are sorted, and
    // `longhands` precedes `shorthands` because sorting the outer keys puts it
    // there, not because of the order they are built in.
    render_group(&mut out, "longhands", longhands.collect());
    out.push_str(",\n");
    render_group(&mut out, "shorthands", shorthands.collect());
    out.push_str("\n}");
    out
}

fn render_group(out: &mut String, name: &str, entries: Vec<(String, Option<String>)>) {
    // A BTreeMap both sorts and collapses duplicates, matching a Python dict
    // built by assignment under `sort_keys=True`.
    let sorted: std::collections::BTreeMap<String, Option<String>> = entries.into_iter().collect();
    let _ = write!(out, "    \"{name}\": {{");
    if sorted.is_empty() {
        out.push('}');
        return;
    }
    out.push('\n');
    let last = sorted.len() - 1;
    for (index, (property, pref)) in sorted.into_iter().enumerate() {
        let _ = write!(out, "        \"{property}\": {{\n            \"pref\": ");
        match pref {
            Some(pref) => {
                let _ = write!(out, "\"{}\"", escape(&pref));
            },
            None => out.push_str("null"),
        }
        out.push_str("\n        }");
        if index != last {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("    }");
}

/// JSON string escaping, for the only characters a preference name can contain.
fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn repository() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn database() -> PropertyDatabase {
        PropertyDatabase::load(&repository().join("style/properties"))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// The generated inventory is byte for byte what `build.py` committed.
    ///
    /// This is the whole test. A near miss is a failure: the point is evidence
    /// that replacing the Python generator changes nothing that ships.
    #[test]
    fn css_properties_json_matches_the_committed_artifact() {
        let expected = std::fs::read_to_string(
            repository().join("style/properties/generated/servo/css-properties.json"),
        )
        .expect("the committed artifact is present");
        let actual = css_properties_json(&database());

        if actual != expected {
            let first_difference = actual
                .lines()
                .zip(expected.lines())
                .enumerate()
                .find(|(_, (left, right))| left != right);
            match first_difference {
                Some((line, (actual_line, expected_line))) => panic!(
                    "line {}:\n  generated: {actual_line}\n  committed: {expected_line}",
                    line + 1
                ),
                None => panic!(
                    "identical for {} lines, then lengths diverge: generated {} lines, committed {}",
                    actual.lines().count().min(expected.lines().count()),
                    actual.lines().count(),
                    expected.lines().count()
                ),
            }
        }
    }
}

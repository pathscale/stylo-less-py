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

use std::collections::BTreeMap;
use std::fmt::Write as _;

use stylo_property_model::{EnabledIn, Engine, PropertyDatabase};

/// The inventory both published artifacts are built from.
///
/// `build.py` builds this dict once and renders it twice, as JSON and as HTML,
/// so building it once here is not a refactor for tidiness: it is what keeps
/// the two outputs from disagreeing.
///
/// A property appears only if it is enabled in content, and each of its aliases
/// appears as its own entry carrying the same preference, because an `Alias`
/// inherits `servo_pref` from the property it aliases. A `BTreeMap` both sorts
/// and collapses duplicates, matching a Python dict built by assignment and
/// dumped under `sort_keys=True`.
type Inventory = BTreeMap<String, Option<String>>;

fn inventory(database: &PropertyDatabase) -> (Inventory, Inventory) {
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
        })
        .collect();
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
        .chain(std::iter::once(("all".to_owned(), None)))
        .collect();
    (longhands, shorthands)
}

/// The property inventory published for rustdoc, as `build.py` writes it.
pub fn css_properties_json(database: &PropertyDatabase) -> String {
    let (longhands, shorthands) = inventory(database);
    let mut out = String::from("{\n");
    // `json.dumps(..., indent=4, sort_keys=True)`. `longhands` precedes
    // `shorthands` because sorting the outer keys puts it there, not because of
    // the order they are built in.
    render_group(&mut out, "longhands", longhands);
    out.push_str(",\n");
    render_group(&mut out, "shorthands", shorthands);
    out.push_str("\n}");
    out
}

/// The same inventory as the rustdoc page `properties.html.mako` renders.
///
/// Mako control lines beginning with `%` emit nothing, so the output is exactly
/// the literal text between them, at the indentation written in the template.
pub fn css_properties_html(database: &PropertyDatabase) -> String {
    let (longhands, shorthands) = inventory(database);
    let mut out = String::from(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         \x20   <meta charset=\"utf-8\">\n\
         \x20   <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
         \x20   <title>Supported CSS properties in Servo</title>\n\
         \x20   <link rel=\"stylesheet\" type=\"text/css\" href=\"../normalize.css\">\n\
         \x20   <link rel=\"stylesheet\" type=\"text/css\" href=\"../rustdoc.css\">\n\
         \x20   <link rel=\"stylesheet\" type=\"text/css\" href=\"../light.css\">\n\
         </head>\n\
         <body class=\"rustdoc\">\n\
         \x20   <section id='main' class=\"content mod\">\n\
         \x20     <h1 class='fqn'><span class='in-band'>CSS properties currently supported in Servo</span></h1>\n",
    );
    // `sorted(properties.items())` orders the two groups by key, which puts
    // longhands first.
    for (kind, entries) in [("Longhands", longhands), ("Shorthands", shorthands)] {
        let _ = write!(
            out,
            "      <h2>{kind}</h2>\n      <table>\n        <tr>\n          <th>Name</th>\n          <th>Pref</th>\n        </tr>\n"
        );
        for (property, pref) in entries {
            let _ = write!(
                out,
                "          <tr>\n            <td><code>{property}</code></td>\n            <td><code>{}</code></td>\n          </tr>\n",
                pref.unwrap_or_default()
            );
        }
        out.push_str("      </table>\n");
    }
    out.push_str("    </section>\n</body>\n</html>\n");
    out
}

fn render_group(out: &mut String, name: &str, sorted: Inventory) {
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

    /// The rustdoc page is byte for byte what `build.py` committed too. Same
    /// standard as the JSON, and the two share their inventory, so a drift in
    /// one is a drift in both.
    #[test]
    fn css_properties_html_matches_the_committed_artifact() {
        let expected = std::fs::read_to_string(
            repository().join("style/properties/generated/servo/css-properties.html"),
        )
        .expect("the committed artifact is present");
        let actual = css_properties_html(&database());

        if actual != expected {
            let first = actual
                .lines()
                .zip(expected.lines())
                .enumerate()
                .find(|(_, (left, right))| left != right);
            match first {
                Some((line, (actual_line, expected_line))) => panic!(
                    "line {}:\n  generated: {actual_line:?}\n  committed: {expected_line:?}",
                    line + 1
                ),
                None => panic!(
                    "prefix matches; lengths differ: generated {} lines, committed {}",
                    actual.lines().count(),
                    expected.lines().count()
                ),
            }
        }
    }
}

//! The CSS property database, as types.
//!
//! `style/properties/data.py` builds this model from the TOML files next to it
//! and hands it to Mako. This crate is that model in Rust, so the generator can
//! be an ordinary build script rather than a vendored Python interpreter.
//!
//! # Why `deny_unknown_fields` is the point
//!
//! `data.py` reads these files with attribute lookups and defaults, so a
//! misspelled key is silently ignored: `servo_pre` instead of `servo_pref`
//! yields a property that quietly ships with no preference gate and no error
//! anywhere. Every struct here rejects unknown fields, which turns that class
//! of typo into a parse failure naming the file, the property and the key.
//!
//! The schema was documented in a 52-line comment at the top of
//! `longhands.toml`, which had already drifted from the data it described: it
//! still called `keyword.values` a space separated string where the file uses
//! an array. The doc comments below are that schema, kept where it cannot
//! drift from the parser.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

/// Everything declared across the property TOMLs.
#[derive(Debug, Clone)]
pub struct PropertyDatabase {
    pub longhands: Vec<Longhand>,
    pub shorthands: Vec<Shorthand>,
    pub counter_style_descriptors: Vec<Descriptor>,
    pub font_face_descriptors: Vec<Descriptor>,
    pub view_transition_descriptors: Vec<Descriptor>,
}

/// One CSS longhand property.
///
/// Field order follows the TOML's own documentation rather than alphabetical,
/// so a reader comparing the two can go down both at once.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Longhand {
    /// The property name, from the TOML section header rather than a field.
    #[serde(skip)]
    pub name: String,

    /// The Rust type name, for example `Display` or `Color`.
    pub r#type: Option<String>,
    /// Which style struct this belongs to: `box`, `font`, `position`, and so on.
    #[serde(rename = "struct")]
    pub style_struct: String,
    /// The engine this property is specific to, if it is specific to one.
    pub engine: Option<Engine>,
    /// URL of the specification.
    pub spec: String,
    /// What the property affects.
    pub affects: Affects,

    /// The computed initial value, as a Rust expression.
    ///
    /// A string because it is code, not data. No serialisation format makes an
    /// embedded expression type-safe, and pretending otherwise would only move
    /// the failure from the generator to the parser.
    pub initial: Option<String>,
    /// Initial specified value, when it differs from `initial`.
    pub initial_specified_value: Option<String>,
    pub keyword: Option<Keyword>,
    pub animation_type: Option<AnimationType>,
    /// Parse method name; `parse` when absent.
    pub parse_method: Option<String>,
    #[serde(default)]
    pub allow_quirks: Option<AllowQuirks>,
    #[serde(default)]
    pub boxed: bool,
    pub vector: Option<Vector>,

    pub gecko_pref: Option<String>,
    pub servo_pref: Option<String>,
    /// Gecko FFI name; defaults to `m` plus the CamelCase name.
    pub gecko_ffi_name: Option<String>,
    pub enabled_in: Option<EnabledIn>,
    #[serde(default)]
    pub logical: bool,
    pub logical_group: Option<String>,
    /// Space-separated flags, for example `CAN_ANIMATE_ON_COMPOSITOR`.
    ///
    /// The last list still encoded inside a string. Parsed into a real list by
    /// [`Longhand::flags`] so the rest of the generator never sees the string.
    #[serde(default)]
    flags: Option<String>,
    /// Aliases, written `"name"` or `"name:pref"`.
    #[serde(default)]
    aliases: Vec<String>,
    /// Additional aliases for Gecko only, not for Servo.
    #[serde(default)]
    extra_gecko_aliases: Vec<String>,
    /// Vendor prefixes, written `"prefix"` or `"prefix:pref"`, each of which
    /// becomes the alias `-<prefix>-<name>`.
    #[serde(default)]
    extra_prefixes: Vec<String>,
    #[serde(default)]
    pub ignored_when_colors_disabled: bool,
    #[serde(default)]
    pub has_effect_on_gecko_scrollbars: Option<bool>,
    #[serde(default)]
    pub rule_types_allowed: Option<Vec<RuleType>>,
    pub servo_restyle_damage: Option<RestyleDamage>,
}

impl Longhand {
    /// The flags as a list, splitting the space-separated string the TOML still
    /// carries. Empty when absent.
    pub fn flags(&self) -> Vec<&str> {
        self.flags
            .as_deref()
            .map(|flags| flags.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// Where the property can be used. `content` when the TOML omits it, which
    /// is what `data.py` defaults the constructor argument to.
    pub fn enabled_in(&self) -> EnabledIn {
        self.enabled_in.unwrap_or(EnabledIn::Content)
    }

    /// Every alias of this property, prefixed forms included.
    ///
    /// `extra_prefixes` is not a separate concept downstream: `data.py` expands
    /// each prefix into an ordinary alias named `-<prefix>-<name>` before
    /// anything reads the list, so expanding it anywhere else invites the two
    /// to drift.
    pub fn aliases(&self) -> Vec<Alias> {
        Alias::expand(&self.name, &self.aliases, &self.extra_prefixes)
    }

    /// Aliases that exist only in a Gecko build.
    pub fn extra_gecko_aliases(&self) -> Vec<Alias> {
        Alias::expand(&self.name, &self.extra_gecko_aliases, &[])
    }

    /// Whether the property is behind a preference for this engine, which is
    /// what `data.py` calls experimental.
    pub fn experimental(&self, engine: Engine) -> bool {
        match engine {
            Engine::Gecko => self.gecko_pref.is_some(),
            Engine::Servo => self.servo_pref.is_some(),
        }
    }

    /// The rule types the property may appear in.
    ///
    /// Absent means `data.py`'s default set rather than none: style, keyframe
    /// and scope. Reading the raw `Option` as "no rules allowed" would silently
    /// drop most properties from the `all` shorthand.
    pub fn rule_types_allowed(&self) -> Vec<RuleType> {
        self.rule_types_allowed
            .clone()
            .unwrap_or_else(|| vec![RuleType::Style, RuleType::Keyframe, RuleType::Scope])
    }
}

/// One alias of a property.
///
/// The TOML writes these as `"name"` or `"name:pref"`, which is the third
/// place structure hides inside a string. Parsed once, here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub name: String,
    /// The Gecko preference gating the alias, when the entry carries one.
    pub gecko_pref: Option<String>,
}

impl Alias {
    fn parse(entry: &str) -> (String, Option<String>) {
        match entry.split_once(':') {
            Some((name, pref)) => (name.to_owned(), Some(pref.to_owned())),
            None => (entry.to_owned(), None),
        }
    }

    fn expand(property: &str, aliases: &[String], extra_prefixes: &[String]) -> Vec<Self> {
        let named = aliases.iter().map(|entry| {
            let (name, gecko_pref) = Self::parse(entry);
            Self { name, gecko_pref }
        });
        let prefixed = extra_prefixes.iter().map(|entry| {
            let (prefix, gecko_pref) = Self::parse(entry);
            Self {
                name: format!("-{prefix}-{property}"),
                gecko_pref,
            }
        });
        named.chain(prefixed).collect()
    }
}

/// One CSS shorthand property.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shorthand {
    #[serde(skip)]
    pub name: String,

    pub sub_properties: Vec<String>,
    /// Absent on exactly one shorthand upstream, so it cannot be required.
    pub spec: Option<String>,
    /// The shorthand's parsing shape, for example `four_sides`.
    pub kind: Option<String>,
    pub engine: Option<Engine>,
    pub gecko_pref: Option<String>,
    pub servo_pref: Option<String>,
    #[serde(default)]
    pub allow_quirks: Option<AllowQuirks>,
    #[serde(default)]
    pub derive_serialize: bool,
    #[serde(default)]
    pub derive_value_info: Option<bool>,
    #[serde(default)]
    pub extra_gecko_sub_properties: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    extra_gecko_aliases: Vec<String>,
    #[serde(default)]
    extra_prefixes: Vec<String>,
    #[serde(default)]
    pub rule_types_allowed: Option<Vec<RuleType>>,
    #[serde(default)]
    flags: Option<String>,
}

impl Shorthand {
    pub fn flags(&self) -> Vec<&str> {
        self.flags
            .as_deref()
            .map(|flags| flags.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// Shorthands carry no `enabled_in` key, so they always take the `content`
    /// default that `data.py` passes for them.
    pub fn enabled_in(&self) -> EnabledIn {
        EnabledIn::Content
    }

    pub fn aliases(&self) -> Vec<Alias> {
        Alias::expand(&self.name, &self.aliases, &self.extra_prefixes)
    }

    pub fn extra_gecko_aliases(&self) -> Vec<Alias> {
        Alias::expand(&self.name, &self.extra_gecko_aliases, &[])
    }
}

/// An at-rule descriptor: `@counter-style`, `@font-face`, `@view-transition`.
///
/// A much smaller schema than a property, and it spells the parse override
/// `parser` where longhands spell it `parse_method`. Kept as upstream has it
/// rather than unified, because renaming a key here would be a change to the
/// data files rather than to this model.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    #[serde(skip)]
    pub name: String,

    pub r#type: String,
    pub parser: Option<String>,
    pub gecko_pref: Option<String>,
}

/// A keyword-valued property's accepted values.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keyword {
    pub values: Vec<String>,
    #[serde(default)]
    pub extra_gecko_values: Vec<String>,
    pub gecko_enum_prefix: Option<String>,
    pub gecko_constant_prefix: Option<String>,
    /// Whether the Gecko enum does not cover all keywords.
    #[serde(default)]
    pub gecko_inexhaustive: bool,
    /// Keyword to enum variant overrides.
    #[serde(default)]
    pub custom_consts: BTreeMap<String, String>,
    /// Keyword-level aliases, Gecko only, written as `"alias=target"`.
    ///
    /// The same wart as `flags`: structure encoded inside a string. Split by
    /// [`Keyword::gecko_aliases`] so the generator never parses it again.
    #[serde(default)]
    gecko_aliases: Vec<String>,
}

impl Keyword {
    /// Gecko keyword aliases as `(alias, target)` pairs.
    pub fn gecko_aliases(&self) -> Vec<(&str, &str)> {
        self.gecko_aliases
            .iter()
            .filter_map(|entry| entry.split_once('='))
            .collect()
    }
}

/// A property whose value is a list.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vector {
    pub animation_type: Option<AnimationType>,
    #[serde(default)]
    pub simple_bindings: Option<bool>,
    #[serde(default)]
    pub need_index: bool,
    /// `Comma` when absent.
    pub separator: Option<String>,
    /// The value a `none` keyword produces, as a Rust expression.
    pub none_value: Option<String>,
}

/// `allow_quirks` is a tri-state upstream, not a boolean: some properties allow
/// quirks only in the shorthand that contains them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum AllowQuirks {
    Always(bool),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Gecko,
    Servo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Affects {
    /// Declared as the empty string upstream, meaning nothing observable.
    #[serde(rename = "")]
    Nothing,
    Layout,
    Overflow,
    Paint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationType {
    None,
    Normal,
    Discrete,
    /// Vector-only variants.
    RepeatableList,
    WithZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnabledIn {
    #[serde(rename = "")]
    Nothing,
    Content,
    Chrome,
    Ua,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleType {
    Style,
    Page,
    Keyframe,
    PositionTry,
    Scope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestyleDamage {
    Repaint,
    RebuildStackingContext,
    RecalculateOverflow,
    RebuildBox,
}

/// A parse failure, carrying enough to find the offending line by hand.
#[derive(Debug)]
pub struct LoadError {
    pub file: String,
    pub source: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.file, self.source)
    }
}

impl std::error::Error for LoadError {}

/// Parse one TOML file of `[name]` sections into a named list.
///
/// The section header carries the property name, which serde cannot see from
/// inside the value, so it is written back onto each record afterwards.
fn load_named<T: for<'de> Deserialize<'de>>(
    path: &Path,
    set_name: fn(&mut T, String),
) -> Result<Vec<T>, LoadError> {
    let fail = |source: String| LoadError {
        file: path.display().to_string(),
        source,
    };
    let text = std::fs::read_to_string(path).map_err(|error| fail(error.to_string()))?;
    let table: BTreeMap<String, T> =
        toml::from_str(&text).map_err(|error| fail(error.to_string()))?;
    Ok(table
        .into_iter()
        .map(|(name, mut value)| {
            set_name(&mut value, name);
            value
        })
        .collect())
}

impl PropertyDatabase {
    /// Load every property TOML from `style/properties`.
    pub fn load(properties_dir: &Path) -> Result<Self, LoadError> {
        Ok(Self {
            longhands: load_named(
                &properties_dir.join("longhands.toml"),
                |value: &mut Longhand, name| value.name = name,
            )?,
            shorthands: load_named(
                &properties_dir.join("shorthands.toml"),
                |value: &mut Shorthand, name| value.name = name,
            )?,
            counter_style_descriptors: load_named(
                &properties_dir.join("counter_style_descriptors.toml"),
                |value: &mut Descriptor, name| value.name = name,
            )?,
            font_face_descriptors: load_named(
                &properties_dir.join("font_face_descriptors.toml"),
                |value: &mut Descriptor, name| value.name = name,
            )?,
            view_transition_descriptors: load_named(
                &properties_dir.join("view_transition_descriptors.toml"),
                |value: &mut Descriptor, name| value.name = name,
            )?,
        })
    }

    /// The longhands an engine actually compiles, which is what every consumer
    /// wants: a property marked for the other engine is not in its build.
    pub fn longhands_for(&self, engine: Engine) -> impl Iterator<Item = &Longhand> {
        self.longhands
            .iter()
            .filter(move |property| property.engine.is_none_or(|only| only == engine))
    }

    /// Members of the synthetic `all` shorthand, in cascade order.
    ///
    /// `all` is not in `shorthands.toml`. `data.py` builds it in
    /// `declare_all_shorthand` because declaring it through the normal helper
    /// generates very large types, so any generator replacing that file has to
    /// build it too or lose a shorthand.
    ///
    /// Logical properties come first, deliberately: physical counterparts
    /// applied first would win, which was
    /// <https://bugzilla.mozilla.org/show_bug.cgi?id=1410028>. Within each
    /// group the sort is by style struct, for cache locality when transitions
    /// iterate the shorthand.
    pub fn all_shorthand_members(&self, engine: Engine) -> Vec<&Longhand> {
        let eligible = |property: &&Longhand| {
            // `direction` and `unicode-bidi` are excluded by the spec: `all`
            // does not reset them.
            !matches!(property.name.as_str(), "direction" | "unicode-bidi")
                && (property.enabled_in() == EnabledIn::Content || property.experimental(engine))
                && property.rule_types_allowed().contains(&RuleType::Style)
        };
        let (mut logical, mut physical): (Vec<&Longhand>, Vec<&Longhand>) = self
            .longhands_for(engine)
            .filter(eligible)
            .partition(|property| property.logical);
        // Stable, so properties within one struct keep declaration order, as
        // Python's sort does.
        //
        // UNVERIFIED against properties.rs: `data.py` sorts on
        // `StyleStruct.name`, which is CamelCase, while this sorts on the
        // TOML's lowercase `struct` value. The two agree for every name in the
        // file today, since case is uniform and no name differs only by
        // separator, but this ordering is observable in the generated `all`
        // shorthand and must be checked byte for byte when properties.rs is
        // generated here.
        logical.sort_by(|left, right| left.style_struct.cmp(&right.style_struct));
        physical.sort_by(|left, right| left.style_struct.cmp(&right.style_struct));
        logical.extend(physical);
        logical
    }

    pub fn shorthands_for(&self, engine: Engine) -> impl Iterator<Item = &Shorthand> {
        self.shorthands
            .iter()
            .filter(move |property| property.engine.is_none_or(|only| only == engine))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> PropertyDatabase {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("style/properties");
        match PropertyDatabase::load(&dir) {
            Ok(database) => database,
            // The whole value of `deny_unknown_fields` is in this message, so
            // do not let a test harness swallow it behind "assertion failed".
            Err(error) => panic!("{error}"),
        }
    }

    /// Every property parses. With `deny_unknown_fields` this also asserts that
    /// no key in any of the five files is one this model does not model.
    #[test]
    fn the_whole_database_parses() {
        let database = database();
        assert_eq!(database.longhands.len(), 429);
        assert_eq!(database.shorthands.len(), 92);
        assert!(!database.counter_style_descriptors.is_empty());
        assert!(!database.font_face_descriptors.is_empty());
        assert!(!database.view_transition_descriptors.is_empty());
    }

    /// Names come from the section headers, not from a field.
    #[test]
    fn properties_keep_their_names() {
        let database = database();
        let transform = database
            .longhands
            .iter()
            .find(|property| property.name == "transform")
            .expect("transform is a longhand");
        assert_eq!(transform.style_struct, "box");
        assert_eq!(transform.affects, Affects::Overflow);
        assert_eq!(transform.flags(), ["CAN_ANIMATE_ON_COMPOSITOR"]);
        assert_eq!(
            transform.servo_restyle_damage,
            Some(RestyleDamage::RecalculateOverflow)
        );
    }

    /// `flags` is the last list still encoded inside a string, so the accessor
    /// splits it. Every value in the database happens to be single today, which
    /// is exactly why the vocabulary is pinned here: a new flag arriving from
    /// upstream should be a failing test rather than a string nothing reads.
    #[test]
    fn flags_parse_and_the_vocabulary_is_known() {
        let database = database();
        let flags: std::collections::BTreeSet<&str> = database
            .longhands
            .iter()
            .flat_map(Longhand::flags)
            .chain(database.shorthands.iter().flat_map(Shorthand::flags))
            .collect();
        assert_eq!(
            flags,
            ["CAN_ANIMATE_ON_COMPOSITOR", "IS_LEGACY_SHORTHAND"]
                .into_iter()
                .collect()
        );
        assert_eq!(
            database
                .longhands
                .iter()
                .filter(|property| property.flags().contains(&"CAN_ANIMATE_ON_COMPOSITOR"))
                .count(),
            11
        );
    }

    /// Gecko keyword aliases are `"alias=target"` pairs inside strings, and the
    /// accessor is the only thing that should ever know that.
    #[test]
    fn gecko_keyword_aliases_split_into_pairs() {
        let database = database();
        let smoothing = database
            .longhands
            .iter()
            .find(|property| property.name == "-moz-osx-font-smoothing")
            .expect("-moz-osx-font-smoothing is a longhand");
        let keyword = smoothing.keyword.as_ref().expect("it is keyword valued");
        assert_eq!(keyword.gecko_aliases(), [("antialiased", "grayscale")]);
    }

    /// A shorthand names longhands that exist. `data.py` resolves these by
    /// lookup and would raise deep inside a template; catching it here names
    /// the shorthand instead.
    #[test]
    fn every_sub_property_resolves_to_a_longhand() {
        let database = database();
        let longhands: std::collections::BTreeSet<&str> = database
            .longhands
            .iter()
            .map(|property| property.name.as_str())
            .collect();
        for shorthand in &database.shorthands {
            for sub_property in &shorthand.sub_properties {
                assert!(
                    longhands.contains(sub_property.as_str()),
                    "shorthand {} names {}, which is not a longhand",
                    shorthand.name,
                    sub_property
                );
            }
        }
    }

    /// An engine's build sees only its own properties plus the shared ones.
    #[test]
    fn engine_filtering_excludes_the_other_engine() {
        let database = database();
        let servo: Vec<&str> = database
            .longhands_for(Engine::Servo)
            .map(|property| property.name.as_str())
            .collect();
        assert!(servo.len() < database.longhands.len());
        assert!(!servo.contains(&"-moz-box-flex"));
    }
}

//! Column lists of ALV layouts that the program may create in SAP (`[layouts.<TCODE>.<NAME>]`).
//!
//! Pure module: no SAP, no console. The sections are read from the raw `config.toml` table
//! (like `[abc.*]` and `[inbond]`):
//!
//! ```toml
//! [layouts.VT11.MY_LAYOUT]
//! columns = ["Shipment Number", "Delivery", "Planned shipment end"]
//! description = "optional, defaults to the layout name"
//! view = "item"   # optional, VL06O only: "item" or "header" (the two views have different columns)
//! ```
//!
//! Column order is the order of the layout, so it is preserved. Blank entries are dropped,
//! duplicates (case-insensitive) are dropped keeping the first.
#![allow(dead_code)]

use std::fmt;

use toml::Value;

/// Which list view a layout belongs to (VL06O: the item view and the header view have different
/// column sets and separate layouts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutView {
    Item,
    Header,
}

impl LayoutView {
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "item" | "items" => Some(Self::Item),
            "header" | "head" => Some(Self::Header),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Header => "header",
        }
    }
}

/// One configured layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutSpec {
    /// Transaction code, upper-cased.
    pub tcode: String,
    /// Layout name as written in the section key.
    pub name: String,
    /// Column titles in layout order (validated: non-empty, no blanks, no duplicates).
    pub columns: Vec<String>,
    /// Description saved with the layout (defaults to the name).
    pub description: String,
    /// `view = "item" | "header"`: the list view the layout is for (checked for VL06O).
    pub view: Option<LayoutView>,
}

/// Why a lookup did not produce a usable layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupError {
    /// No `[layouts.<TCODE>.<NAME>]` section; `configured` lists the layout names of that tcode.
    NotConfigured {
        tcode: String,
        name: String,
        configured: Vec<String>,
    },
    /// The section exists but is not valid (empty column list, wrong types, ...).
    Invalid {
        tcode: String,
        name: String,
        problem: String,
    },
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LookupError::NotConfigured {
                tcode,
                name,
                configured,
            } => {
                write!(f, "no [layouts.{tcode}.{name}] section in config.toml")?;
                if configured.is_empty() {
                    write!(f, " (no layouts are configured for {tcode})")
                } else {
                    write!(f, " (configured for {tcode}: {})", configured.join(", "))
                }
            }
            LookupError::Invalid {
                tcode,
                name,
                problem,
            } => write!(f, "[layouts.{tcode}.{name}] is invalid: {problem}"),
        }
    }
}

impl std::error::Error for LookupError {}

/// Trim, drop blanks and case-insensitive duplicates (first wins), keep the order.
pub fn normalize_columns<S: AsRef<str>>(raw: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in raw {
        let c = c.as_ref().trim();
        if c.is_empty() {
            continue;
        }
        if out.iter().any(|e| e.eq_ignore_ascii_case(c)) {
            continue;
        }
        out.push(c.to_string());
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Invalid {
    tcode: String,
    name: String,
    problem: String,
}

/// All `[layouts.*]` sections of a config file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutStore {
    entries: Vec<LayoutSpec>,
    invalid: Vec<Invalid>,
}

impl LayoutStore {
    /// Parse the `layouts` table of the raw config root (`None` / no section = empty store).
    pub fn from_value(root: Option<&Value>) -> Self {
        let mut store = LayoutStore::default();
        let Some(layouts) = root
            .and_then(|r| r.get("layouts"))
            .and_then(Value::as_table)
        else {
            return store;
        };
        for (tcode, per_tcode) in layouts {
            let tcode_u = tcode.trim().to_uppercase();
            let Some(per_tcode) = per_tcode.as_table() else {
                store.invalid.push(Invalid {
                    tcode: tcode_u,
                    name: String::new(),
                    problem: "expected sub-tables [layouts.<TCODE>.<NAME>]".to_string(),
                });
                continue;
            };
            for (name, section) in per_tcode {
                let name_t = name.trim().to_string();
                match parse_section(section) {
                    Ok((columns, description, view)) => store.entries.push(LayoutSpec {
                        tcode: tcode_u.clone(),
                        description: description.unwrap_or_else(|| name_t.clone()),
                        name: name_t,
                        columns,
                        view,
                    }),
                    Err(problem) => store.invalid.push(Invalid {
                        tcode: tcode_u.clone(),
                        name: name_t,
                        problem,
                    }),
                }
            }
        }
        store
    }

    /// Parse TOML text (tests, tools).
    pub fn from_toml_str(text: &str) -> Result<Self, String> {
        let v: Value = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self::from_value(Some(&v)))
    }

    /// Load from `config.toml` (same location logic as every other section). A missing or
    /// unreadable file gives an empty store.
    pub fn load() -> Self {
        match crate::utils::config_types::SapConfig::load() {
            Ok(cfg) => Self::from_value(cfg.raw_config.as_ref()),
            Err(_) => Self::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.invalid.is_empty()
    }

    /// Usable layouts of `tcode` (case-insensitive), in config order.
    pub fn for_tcode(&self, tcode: &str) -> Vec<&LayoutSpec> {
        let t = tcode.trim();
        self.entries
            .iter()
            .filter(|e| e.tcode.eq_ignore_ascii_case(t))
            .collect()
    }

    /// Names of the usable layouts of `tcode`.
    pub fn names_for(&self, tcode: &str) -> Vec<String> {
        self.for_tcode(tcode)
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    /// Tcodes that have at least one usable layout, upper-cased, sorted.
    pub fn tcodes(&self) -> Vec<String> {
        let mut v: Vec<String> = self.entries.iter().map(|e| e.tcode.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Find `[layouts.<tcode>.<name>]` (tcode and name case-insensitive, trimmed).
    pub fn get(&self, tcode: &str, name: &str) -> Option<&LayoutSpec> {
        let (t, n) = (tcode.trim(), name.trim());
        self.entries
            .iter()
            .find(|e| e.tcode.eq_ignore_ascii_case(t) && e.name.eq_ignore_ascii_case(n))
    }

    /// Like [`get`](Self::get) with an error that explains what is wrong or what is configured.
    pub fn lookup(&self, tcode: &str, name: &str) -> Result<&LayoutSpec, LookupError> {
        if let Some(spec) = self.get(tcode, name) {
            return Ok(spec);
        }
        let (t, n) = (tcode.trim(), name.trim());
        if let Some(bad) = self
            .invalid
            .iter()
            .find(|i| i.tcode.eq_ignore_ascii_case(t) && i.name.eq_ignore_ascii_case(n))
        {
            return Err(LookupError::Invalid {
                tcode: bad.tcode.clone(),
                name: bad.name.clone(),
                problem: bad.problem.clone(),
            });
        }
        Err(LookupError::NotConfigured {
            tcode: t.to_uppercase(),
            name: n.to_string(),
            configured: self.names_for(t),
        })
    }
}

type Section = (Vec<String>, Option<String>, Option<LayoutView>);

fn parse_section(section: &Value) -> Result<Section, String> {
    let table = section
        .as_table()
        .ok_or_else(|| "expected a table with `columns = [...]`".to_string())?;
    let cols = table
        .get("columns")
        .ok_or_else(|| "missing `columns = [...]`".to_string())?
        .as_array()
        .ok_or_else(|| "`columns` must be an array of strings".to_string())?;
    let mut raw: Vec<String> = Vec::new();
    for c in cols {
        match c.as_str() {
            Some(s) => raw.push(s.to_string()),
            None => return Err("`columns` must only contain strings".to_string()),
        }
    }
    let columns = normalize_columns(&raw);
    if columns.is_empty() {
        return Err("`columns` is empty".to_string());
    }
    let description = match table.get("description") {
        None => None,
        Some(v) => match v.as_str() {
            Some(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            Some(_) => None,
            None => return Err("`description` must be a string".to_string()),
        },
    };
    let view =
        match table.get("view") {
            None => None,
            Some(v) => match v.as_str() {
                Some(s) if s.trim().is_empty() => None,
                Some(s) => Some(LayoutView::parse(s).ok_or_else(|| {
                    format!("`view` must be \"item\" or \"header\" (got \"{s}\")")
                })?),
                None => return Err("`view` must be a string".to_string()),
            },
        };
    Ok((columns, description, view))
}

/// `[global] create_missing_layout` of the raw config root (bool, or the strings `true`/`1`/`yes`/
/// `on`); `None` when absent.
pub fn global_create_missing_layout(root: Option<&Value>) -> Option<bool> {
    let v = root?.get("global")?.get("create_missing_layout")?;
    match v {
        Value::Boolean(b) => Some(*b),
        Value::String(s) => Some(matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        )),
        Value::Integer(i) => Some(*i != 0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[layouts.VT11.EXAMPLE_ONE]
columns = ["Shipment Number", " Delivery ", "", "delivery", "Planned shipment end"]
description = "Example one"

[layouts.VT11.example_two]
columns = ["A", "B"]

[layouts.zmdesnr.EXAMPLE_THREE]
columns = ["Plant"]

[layouts.LX03.BAD_EMPTY]
columns = ["", "  "]

[layouts.LX03.BAD_TYPE]
columns = "Plant"

[layouts.LX03.BAD_MISSING]
description = "x"
"#;

    fn store() -> LayoutStore {
        LayoutStore::from_toml_str(SAMPLE).unwrap()
    }

    #[test]
    fn parses_and_validates_columns() {
        let s = store();
        let one = s.get("VT11", "EXAMPLE_ONE").unwrap();
        // blanks dropped, case-insensitive duplicate dropped, order kept
        assert_eq!(
            one.columns,
            ["Shipment Number", "Delivery", "Planned shipment end"]
        );
        assert_eq!(one.description, "Example one");
        // description defaults to the name
        assert_eq!(
            s.get("VT11", "example_two").unwrap().description,
            "example_two"
        );
    }

    #[test]
    fn lookup_is_case_insensitive_and_trimmed() {
        let s = store();
        assert!(s.get("vt11", "  example_one ").is_some());
        assert!(s.get("Zmdesnr", "EXAMPLE_THREE").is_some());
        assert_eq!(s.get("ZMDESNR", "example_three").unwrap().tcode, "ZMDESNR");
        assert!(s.get("VT11", "nope").is_none());
        assert!(s.get("VL06O", "EXAMPLE_ONE").is_none());
    }

    #[test]
    fn lists_by_tcode() {
        let s = store();
        assert_eq!(s.names_for("vt11"), ["EXAMPLE_ONE", "example_two"]);
        assert_eq!(s.tcodes(), ["VT11", "ZMDESNR"]);
        assert!(s.names_for("LX03").is_empty());
    }

    #[test]
    fn invalid_sections_are_reported_not_used() {
        let s = store();
        for n in ["BAD_EMPTY", "BAD_TYPE", "BAD_MISSING"] {
            assert!(s.get("LX03", n).is_none(), "{n}");
            match s.lookup("LX03", n) {
                Err(LookupError::Invalid { problem, .. }) => assert!(!problem.is_empty()),
                other => panic!("{n}: {other:?}"),
            }
        }
    }

    #[test]
    fn missing_section_error_lists_what_is_configured() {
        let e = store().lookup("VT11", "nope").unwrap_err().to_string();
        assert!(e.contains("[layouts.VT11.nope]"), "{e}");
        assert!(
            e.contains("EXAMPLE_ONE") && e.contains("example_two"),
            "{e}"
        );
        let e = store().lookup("VL06O", "x").unwrap_err().to_string();
        assert!(e.contains("no layouts are configured for VL06O"), "{e}");
    }

    #[test]
    fn no_section_is_an_empty_store() {
        let s = LayoutStore::from_toml_str("[global]\nreports_dir = \"x\"\n").unwrap();
        assert!(s.is_empty());
        assert!(LayoutStore::from_value(None).is_empty());
    }

    #[test]
    fn normalize_keeps_order() {
        assert_eq!(normalize_columns(&["b", "a", "B", " "]), ["b", "a"]);
    }

    #[test]
    fn global_flag_accepts_bool_and_string() {
        let t = |s: &str| {
            let v: Value = toml::from_str(s).unwrap();
            global_create_missing_layout(Some(&v))
        };
        assert_eq!(t("[global]\ncreate_missing_layout = true"), Some(true));
        assert_eq!(t("[global]\ncreate_missing_layout = false"), Some(false));
        assert_eq!(t("[global]\ncreate_missing_layout = \"yes\""), Some(true));
        assert_eq!(t("[global]\ncreate_missing_layout = \"no\""), Some(false));
        assert_eq!(t("[global]\nreports_dir = \"x\""), None);
        assert_eq!(t("[tcode.X]\na = 1"), None);
    }

    #[test]
    fn survives_a_config_save_round_trip() {
        use crate::utils::config_types::SapConfig;
        let text = format!(
            "[global]\nreports_dir = \"C:\\\\r\"\ncreate_missing_layout = true\n\n{SAMPLE}\n[tcode.VT11]\nlayout = \"x\"\n"
        );
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("in.toml");
        std::fs::write(&src, &text).unwrap();
        let cfg = SapConfig::load_from_path(src.to_str().unwrap()).unwrap();
        let before = LayoutStore::from_value(cfg.raw_config.as_ref());
        assert!(!before.is_empty());

        let out = dir.path().join("out.toml");
        cfg.save_to_path(out.to_str().unwrap()).unwrap();
        let written = std::fs::read_to_string(&out).unwrap();

        let reloaded = SapConfig::load_from_path(out.to_str().unwrap()).unwrap();
        let after = LayoutStore::from_value(reloaded.raw_config.as_ref());
        assert_eq!(before, after, "layouts changed by a save:\n{written}");
        assert_eq!(
            after.get("VT11", "EXAMPLE_ONE").unwrap().columns,
            ["Shipment Number", "Delivery", "Planned shipment end"]
        );
        assert_eq!(
            global_create_missing_layout(reloaded.raw_config.as_ref()),
            Some(true),
            "[global] create_missing_layout lost by a save:\n{written}"
        );
    }

    #[test]
    fn view_is_parsed_and_validated() {
        let s = LayoutStore::from_toml_str(concat!(
            "[layouts.VL06O.A]\ncolumns = [\"Delivery\"]\nview = \"Item\"\n",
            "[layouts.VL06O.B]\ncolumns = [\"Delivery\"]\nview = \"header\"\n",
            "[layouts.VL06O.C]\ncolumns = [\"Delivery\"]\n",
            "[layouts.VL06O.D]\ncolumns = [\"Delivery\"]\nview = \"sideways\"\n",
        ))
        .unwrap();
        assert_eq!(s.get("VL06O", "A").unwrap().view, Some(LayoutView::Item));
        assert_eq!(s.get("VL06O", "B").unwrap().view, Some(LayoutView::Header));
        assert_eq!(s.get("VL06O", "C").unwrap().view, None);
        assert!(s.get("VL06O", "D").is_none());
        assert!(matches!(
            s.lookup("VL06O", "D"),
            Err(LookupError::Invalid { .. })
        ));
    }
}

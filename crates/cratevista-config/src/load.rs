//! Reading and parsing the discovered files.
//!
//! Every failure here is **per-file and non-fatal**: an unreadable or malformed
//! file becomes a diagnostic and the remaining files still load. One broken flow
//! must not cost a project its whole configuration.
//!
//! The file's text is retained alongside the parsed value so byte spans can be
//! resolved to line/column later, without re-reading from disk.

use std::path::Path;

use crate::discover::{Discovered, relative_label};
use crate::error::{ConfigDiagnostic, code};
use crate::model::{LoadedFile, RawConfig, RawFlowFile, RawOverrideFile, RawRootConfig};

/// Parses `text` into `T`, mapping a TOML error onto a located diagnostic.
///
/// `toml::de::Error` carries a byte span when it can; when it cannot, the
/// diagnostic degrades to file-level rather than inventing a position.
fn parse<T: serde::de::DeserializeOwned>(text: &str, label: &str) -> Result<T, ConfigDiagnostic> {
    toml::from_str::<T>(text).map_err(|error| {
        // `deny_unknown_fields` + missing/mistyped fields all surface here, so
        // distinguish a syntax error from a shape error for an actionable code.
        let message = error.message().to_string();
        let is_structural = message.contains("unknown field")
            || message.contains("missing field")
            || message.contains("invalid type")
            || message.contains("invalid length")
            || message.contains("duplicate key");
        let diagnostic = ConfigDiagnostic::new(
            if is_structural {
                code::INVALID_STRUCTURE
            } else {
                code::PARSE_ERROR
            },
            message,
            label,
        );
        match error.span() {
            Some(span) => diagnostic.at(text, span),
            None => diagnostic,
        }
    })
}

/// Reads and parses one file, returning either the loaded value or a diagnostic.
fn load_file<T: serde::de::DeserializeOwned>(
    workspace_root: &Path,
    path: &Path,
) -> Result<LoadedFile<T>, ConfigDiagnostic> {
    let label = relative_label(workspace_root, path);
    let source = std::fs::read_to_string(path).map_err(|error| {
        // The io error may name an absolute path; report our relative label and
        // the kind only, never the raw OS message's path.
        ConfigDiagnostic::new(
            code::READ_FAILED,
            format!("could not read the file: {}", error.kind()),
            &label,
        )
    })?;
    let value = parse::<T>(&source, &label)?;
    Ok(LoadedFile {
        path: label,
        source,
        value,
    })
}

/// Loads everything `discover` found. Never fails as a whole: problems are
/// collected as diagnostics on [`RawConfig::diagnostics`].
pub fn load(workspace_root: &Path, discovered: &Discovered) -> RawConfig {
    let mut config = RawConfig::default();

    if let Some(root) = &discovered.root {
        match load_file::<RawRootConfig>(workspace_root, root) {
            Ok(file) => config.root = Some(file.value),
            Err(diagnostic) => config.diagnostics.push(diagnostic),
        }
    }
    for path in &discovered.flows {
        match load_file::<RawFlowFile>(workspace_root, path) {
            Ok(file) => config.flow_files.push(file),
            Err(diagnostic) => config.diagnostics.push(diagnostic),
        }
    }
    for path in &discovered.overrides {
        match load_file::<RawOverrideFile>(workspace_root, path) {
            Ok(file) => config.override_files.push(file),
            Err(diagnostic) => config.diagnostics.push(diagnostic),
        }
    }
    config
}

/// Discovers and loads in one step.
pub fn load_from(workspace_root: &Path) -> RawConfig {
    load(workspace_root, &crate::discover::discover(workspace_root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RawLocalized;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn no_configuration_loads_an_empty_set() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(dir.path());
        assert!(config.is_empty());
        assert!(config.diagnostics.is_empty(), "absence is not a problem");
    }

    #[test]
    fn parses_a_flow_file_with_entities_flows_stages_relations_and_examples() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/checkout.toml",
            r#"
[[entity]]
id = "redis"
kind = "infrastructure"
label = "Redis"
description = { default = "Cache", de = "Zwischenspeicher" }
tags = ["infra"]

[[flow]]
id = "checkout"
title = "Checkout"
members = ["manual:redis"]
default_focus = "manual:redis"
docs = ["docs/checkout.md"]

  [[flow.stage]]
  id = "clients"
  title = "Clients"
  order = 1

  [[flow.relation]]
  from = "manual:redis"
  to = "manual:redis"
  label = "SQL"

  [[flow.example]]
  id = "req"
  title = "Request"
  path = "examples/req.http"
  language = "http"
"#,
        );

        let config = load_from(dir.path());
        assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
        assert_eq!(config.flow_files.len(), 1);

        let file = &config.flow_files[0];
        assert_eq!(file.path, ".cratevista/flows/checkout.toml");
        assert_eq!(file.value.entities.len(), 1);
        assert_eq!(file.value.entities[0].id.get_ref(), "redis");

        let entity = &file.value.entities[0];
        assert!(matches!(entity.label, RawLocalized::Plain(ref s) if s == "Redis"));
        // A translation table round-trips too.
        match entity.description.as_ref().unwrap() {
            RawLocalized::Translations(map) => {
                assert_eq!(map.get("default").unwrap(), "Cache");
                assert_eq!(map.get("de").unwrap(), "Zwischenspeicher");
            }
            other => panic!("expected translations, got {other:?}"),
        }

        let flow = &file.value.flows[0];
        assert_eq!(flow.id.get_ref(), "checkout");
        assert_eq!(flow.members.len(), 1);
        assert_eq!(flow.stages.len(), 1);
        assert_eq!(flow.relations.len(), 1);
        assert_eq!(flow.examples.len(), 1);
        assert_eq!(flow.examples[0].language.as_deref(), Some("http"));
        assert_eq!(flow.docs.len(), 1);
    }

    #[test]
    fn comments_are_supported() {
        // Reviewable, comment-friendly config is a stated issue-08 requirement.
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "# why this flow exists\n[[flow]]\nid = \"a\" # the id\ntitle = \"A\"\n",
        );
        let config = load_from(dir.path());
        assert!(config.diagnostics.is_empty());
        assert_eq!(config.flow_files[0].value.flows[0].id.get_ref(), "a");
    }

    #[test]
    fn a_syntax_error_is_located_and_costs_only_its_own_file() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/aaa_broken.toml",
            "this is not = = toml\n",
        );
        write(
            dir.path(),
            ".cratevista/flows/zzz_good.toml",
            "[[flow]]\nid = \"ok\"\ntitle = \"Ok\"\n",
        );

        let config = load_from(dir.path());
        assert_eq!(config.diagnostics.len(), 1);
        let diagnostic = &config.diagnostics[0];
        assert_eq!(diagnostic.code, code::PARSE_ERROR);
        assert_eq!(diagnostic.file, ".cratevista/flows/aaa_broken.toml");
        assert!(
            diagnostic.position.is_some(),
            "a syntax error has a position"
        );

        // The healthy file still loaded.
        assert_eq!(config.flow_files.len(), 1);
        assert_eq!(config.flow_files[0].path, ".cratevista/flows/zzz_good.toml");
    }

    #[test]
    fn a_missing_required_field_is_a_located_structural_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        // `title` is required.
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"a\"\n",
        );
        let config = load_from(dir.path());
        assert_eq!(config.diagnostics.len(), 1);
        assert_eq!(config.diagnostics[0].code, code::INVALID_STRUCTURE);
        assert!(config.diagnostics[0].message.contains("missing field"));
        assert!(config.diagnostics[0].position.is_some());
    }

    #[test]
    fn an_unknown_field_is_rejected_rather_than_silently_ignored() {
        // A typo'd key must not be swallowed: `deny_unknown_fields` turns
        // `titel = "…"` into an actionable error instead of a missing title.
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = \"a\"\ntitle = \"A\"\ntitel = \"typo\"\n",
        );
        let config = load_from(dir.path());
        assert_eq!(config.diagnostics.len(), 1);
        assert_eq!(config.diagnostics[0].code, code::INVALID_STRUCTURE);
        assert!(config.diagnostics[0].message.contains("unknown field"));
    }

    #[test]
    fn a_wrong_type_is_a_structural_diagnostic() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/flows/a.toml",
            "[[flow]]\nid = 42\ntitle = \"A\"\n",
        );
        let config = load_from(dir.path());
        assert_eq!(config.diagnostics[0].code, code::INVALID_STRUCTURE);
        assert!(config.diagnostics[0].message.contains("invalid type"));
    }

    #[test]
    fn the_root_config_parses_and_reserved_sections_are_tolerated_but_ignored() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "cratevista.toml",
            r#"
version = "1"

[metadata]
include_external_deps = false

[rustdoc]
document_private_items = false

[server]
port = 7420
"#,
        );
        let config = load_from(dir.path());
        assert!(
            config.diagnostics.is_empty(),
            "reserved sections must parse"
        );
        let root = config.root.unwrap();
        assert_eq!(root.version.as_deref(), Some("1"));
        // Captured but deliberately unbound in the MVP.
        assert!(root.metadata.is_some());
        assert!(root.rustdoc.is_some());
        assert!(root.server.is_some());
    }

    #[test]
    fn files_load_in_sorted_order() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["c.toml", "a.toml", "b.toml"] {
            let id = name.trim_end_matches(".toml");
            write(
                dir.path(),
                &format!(".cratevista/flows/{name}"),
                &format!("[[flow]]\nid = \"{id}\"\ntitle = \"{id}\"\n"),
            );
        }
        let config = load_from(dir.path());
        let paths: Vec<&str> = config.flow_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                ".cratevista/flows/a.toml",
                ".cratevista/flows/b.toml",
                ".cratevista/flows/c.toml"
            ]
        );
    }

    #[test]
    fn overrides_parse_with_all_documented_keys() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            ".cratevista/overrides/api.toml",
            r#"
[[override]]
target = "item:struct:cvcore::model::Widget"
label = "Widget"
description = "The widget"
tags = ["featured"]
category = "core"
stage = "stage:clients"
hidden = false
promoted = true
docs = ["docs/widget.md"]

  [override.presentation]
  color = "blue"
"#,
        );
        let config = load_from(dir.path());
        assert!(config.diagnostics.is_empty(), "{:?}", config.diagnostics);
        let entry = &config.override_files[0].value.overrides[0];
        assert_eq!(entry.target.get_ref(), "item:struct:cvcore::model::Widget");
        assert_eq!(entry.category.as_deref(), Some("core"));
        assert_eq!(entry.promoted, Some(true));
        assert_eq!(entry.docs.len(), 1);
        assert!(entry.presentation.contains_key("color"));
    }

    #[test]
    fn no_diagnostic_leaks_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".cratevista/flows/broken.toml", "= bad\n");
        let config = load_from(dir.path());
        let root_text = dir.path().to_string_lossy().to_string();
        for diagnostic in &config.diagnostics {
            assert!(
                !diagnostic.file.contains(&root_text) && !diagnostic.message.contains(&root_text),
                "diagnostic leaked an absolute path: {diagnostic}"
            );
        }
    }
}

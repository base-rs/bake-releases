// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

//! The release settings read from the project's root `Cargo.toml`.

use bake::{Error, Result};
use std::io::ErrorKind;
use std::path::Path;
use toml_edit::{DocumentMut, Item};

/// Release settings, with their defaults when the manifest leaves them out.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Project {
    /// `package.repository`, or else `workspace.package.repository`.
    pub(crate) repository: Option<String>,
    /// `bake.release.tag-prefix` metadata, `v` by default, as `bake-cargo` reads it.
    pub(crate) tag_prefix: String,
    /// `bake.release.notes-exclude` metadata from the workspace and the package.
    pub(crate) notes_exclude: Vec<String>,
}

fn release<'document>(document: &'document DocumentMut, section: &str) -> Option<&'document Item> {
    document
        .get(section)?
        .get("metadata")?
        .get("bake")?
        .get("release")
}

impl Project {
    pub(crate) fn read(root: &Path) -> Result<Self> {
        let path = root.join("Cargo.toml");
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
            Err(error) => return Err(Error::new(format!("{}: {error}", path.display()))),
        };
        let document = source
            .parse::<DocumentMut>()
            .map_err(|error| Error::new(format!("could not parse {}: {error}", path.display())))?;
        Self::from_document(&document)
    }

    fn from_document(document: &DocumentMut) -> Result<Self> {
        let repository = document
            .get("package")
            .and_then(|package| package.get("repository"))
            .and_then(Item::as_str)
            .or_else(|| {
                document
                    .get("workspace")?
                    .get("package")?
                    .get("repository")?
                    .as_str()
            })
            .map(str::to_owned);
        let sections = ["workspace", "package"].map(|section| release(document, section));
        let tag_prefix = match sections
            .iter()
            .flatten()
            .find_map(|item| item.get("tag-prefix"))
        {
            Some(value) => value
                .as_str()
                .ok_or_else(|| {
                    Error::new("Cargo metadata bake.release.tag-prefix must be a string")
                })?
                .to_owned(),
            None => "v".to_owned(),
        };
        let mut notes_exclude = Vec::new();
        for item in sections
            .iter()
            .flatten()
            .filter_map(|item| item.get("notes-exclude"))
        {
            let paths = item.as_array().and_then(|paths| {
                paths
                    .iter()
                    .map(|path| path.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
            });
            notes_exclude.extend(paths.ok_or_else(|| {
                Error::new("Cargo metadata bake.release.notes-exclude must be an array of strings")
            })?);
        }
        Ok(Self {
            repository,
            tag_prefix,
            notes_exclude,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(source: &str) -> Result<Project> {
        Project::from_document(&source.parse().unwrap())
    }

    #[test]
    fn reads_defaults_from_manifests_without_settings() {
        for source in [
            "",
            "[package]\nname = \"a\"\n",
            "[package.metadata.other]\n",
            "[package.metadata.bake]\n",
            "[workspace]\n",
            "[workspace.package]\nversion = \"0.1.0\"\n",
        ] {
            assert_eq!(
                project(source).unwrap(),
                Project {
                    repository: None,
                    tag_prefix: "v".to_owned(),
                    notes_exclude: Vec::new(),
                },
                "{source:?}"
            );
        }
    }

    #[test]
    fn reads_package_settings_before_workspace_settings() {
        let source = r#"
[package]
repository = "https://github.com/base-rs/package"
metadata.bake.release.notes-exclude = ["examples"]

[workspace.package]
repository = "https://github.com/base-rs/workspace"

[workspace.metadata.bake.release]
tag-prefix = "base-v"
notes-exclude = ["benchmarks"]
"#;
        assert_eq!(
            project(source).unwrap(),
            Project {
                repository: Some("https://github.com/base-rs/package".to_owned()),
                tag_prefix: "base-v".to_owned(),
                notes_exclude: vec!["benchmarks".to_owned(), "examples".to_owned()],
            }
        );
        let inherited = "[package]\nrepository.workspace = true\n[workspace.package]\nrepository = \"https://github.com/base-rs/workspace\"\n";
        assert_eq!(
            project(inherited).unwrap().repository.as_deref(),
            Some("https://github.com/base-rs/workspace")
        );
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(project("[workspace.metadata.bake.release]\ntag-prefix = 1\n").is_err());
        assert!(project("[package.metadata.bake.release]\nnotes-exclude = \"x\"\n").is_err());
        assert!(project("[package.metadata.bake.release]\nnotes-exclude = [1]\n").is_err());
    }

    #[test]
    fn reads_a_missing_manifest_as_empty_and_reports_invalid_ones() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            Project::read(directory.path()).unwrap(),
            project("").unwrap()
        );
        std::fs::write(directory.path().join("Cargo.toml"), "[").unwrap();
        assert!(Project::read(directory.path()).is_err());
        std::fs::remove_file(directory.path().join("Cargo.toml")).unwrap();
        std::fs::create_dir(directory.path().join("Cargo.toml")).unwrap();
        assert!(Project::read(directory.path()).is_err());
    }
}

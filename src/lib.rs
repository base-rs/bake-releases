// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

//! Reusable release-document tasks, inspired by Samuel Williams's Ruby
//! `bake-releases`: <https://github.com/ioquatix/bake-releases> (MIT).
//! This implementation preserves Markdown bytes rather than re-rendering them,
//! and generates each version's notes from the squash-merged commits since the
//! previous release tag, so nobody edits `releases.md` by hand.
mod document;
mod history;
mod project;

#[allow(deprecated)]
pub use document::update_document;
pub use document::{Commit, Notes, extract_notes, insert_notes, render_section};

use bake::{Context, Error, Result, Value};
use history::Version;
use project::Project;
use std::collections::BTreeMap;
use std::fs;
use std::fs::{File, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

fn persist_file(temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| Error::from(error.error))
}

trait UpdateIo {
    fn create_temporary(&mut self, directory: &Path) -> io::Result<tempfile::NamedTempFile>;
    fn write(&mut self, file: &mut File, contents: &[u8]) -> io::Result<()>;
    fn permissions(&mut self, path: &Path) -> io::Result<Permissions>;
    fn set_permissions(&mut self, file: &File, permissions: Permissions) -> io::Result<()>;
    fn sync_all(&mut self, file: &File) -> io::Result<()>;
    fn persist(&mut self, temporary: tempfile::NamedTempFile, path: &Path) -> Result<()>;
}

struct SystemUpdateIo;

impl UpdateIo for SystemUpdateIo {
    fn create_temporary(&mut self, directory: &Path) -> io::Result<tempfile::NamedTempFile> {
        tempfile::NamedTempFile::new_in(directory)
    }

    fn write(&mut self, file: &mut File, contents: &[u8]) -> io::Result<()> {
        file.write_all(contents)
    }

    fn permissions(&mut self, path: &Path) -> io::Result<Permissions> {
        fs::metadata(path).map(|metadata| metadata.permissions())
    }

    fn set_permissions(&mut self, file: &File, permissions: Permissions) -> io::Result<()> {
        file.set_permissions(permissions)
    }

    fn sync_all(&mut self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn persist(&mut self, temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
        persist_file(temporary, path)
    }
}

fn update_file_with(
    directory: &Path,
    path: &Path,
    updated: &str,
    operations: &mut impl UpdateIo,
) -> Result<()> {
    let mut temporary = operations.create_temporary(directory)?;
    operations.write(temporary.as_file_mut(), updated.as_bytes())?;
    let permissions = operations.permissions(path)?;
    operations.set_permissions(temporary.as_file(), permissions)?;
    operations.sync_all(temporary.as_file())?;
    operations.persist(temporary, path)?;
    Ok(())
}

/// The options shared by the tasks that generate notes from Git history.
struct Options {
    since: Option<String>,
    scope: Vec<String>,
    exclude: Vec<String>,
    repository: Option<String>,
}

/// The commits in a release range and the notes generated from them.
struct Generated {
    since: Option<String>,
    commits: Vec<Commit>,
    notes: Notes,
}

/// Generate the notes for the first-parent history of `until` since the previous
/// release tag, which must be below `version` when the version is semantic.
fn generate(
    context: &Context,
    version: Option<&str>,
    until: &str,
    options: Options,
) -> Result<Generated> {
    let project = Project::read(context.root())?;
    let since = match options.since {
        Some(since) => Some(since),
        None => {
            let below = version
                .and_then(|version| version.strip_prefix(&project.tag_prefix))
                .and_then(Version::parse);
            history::previous_tag(context, &project.tag_prefix, until, below.as_ref())?
        }
    };
    let mut excluded = project.notes_exclude;
    excluded.extend(options.exclude);
    let pathspec = history::pathspec(&options.scope, &excluded);
    let commits = history::commits(context, since.as_deref(), until, &pathspec)?;
    let repository = options.repository.or(project.repository);
    let notes = Notes::from_commits(&commits, repository.as_deref());
    Ok(Generated {
        since,
        commits,
        notes,
    })
}

impl Generated {
    fn require_commits(self) -> Result<Self> {
        if self.commits.is_empty() {
            let since = self.since.as_deref().unwrap_or("the start of history");
            return Err(Error::new(format!(
                "no commits in the release scope since {since}; there is nothing to release"
            )));
        }
        Ok(self)
    }
}

fn read_document(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|error| Error::new(format!("{}: {error}", path.display())))
}

/// Extract the Markdown body beneath an exact release heading. `Unreleased`
/// previews the next version's notes, generated from Git history, without
/// writing anything.
#[bake::task]
#[allow(clippy::too_many_arguments)]
pub fn notes(
    context: &mut Context,
    version: String,
    #[bake(
        default = "releases.md",
        help = "Release document relative to the project root."
    )]
    path: PathBuf,
    #[bake(help = "Revision of the previous release; by default, the newest earlier release tag.")]
    since: Option<String>,
    #[bake(help = "Git pathspec of the commits that count; by default, the whole repository.")]
    scope: Vec<String>,
    #[bake(help = "Path, relative to the repository root, whose commits do not count.")]
    exclude: Vec<String>,
    #[bake(help = "Repository URL for links; by default, the manifest's `repository`.")]
    repository: Option<String>,
) -> Result<String> {
    let document = read_document(&context.root().join(path))?;
    if version == "Unreleased" {
        let options = Options {
            since,
            scope,
            exclude,
            repository,
        };
        let generated = generate(context, None, "HEAD", options)?;
        let kept = extract_notes(&document, "Unreleased").unwrap_or_default();
        return Ok(generated.notes.render(kept));
    }
    Ok(extract_notes(&document, &version)?.to_owned())
}

/// Write the `## VERSION` section, generated from the commits since the previous
/// release tag, keeping the body of any `## Unreleased` section.
#[bake::task]
#[allow(clippy::too_many_arguments)]
pub fn update(
    context: &mut Context,
    version: String,
    #[bake(default = "releases.md")] path: PathBuf,
    #[bake(help = "Revision of the previous release; by default, the newest earlier release tag.")]
    since: Option<String>,
    #[bake(help = "Git pathspec of the commits that count; by default, the whole repository.")]
    scope: Vec<String>,
    #[bake(help = "Path, relative to the repository root, whose commits do not count.")]
    exclude: Vec<String>,
    #[bake(help = "Repository URL for links; by default, the manifest's `repository`.")]
    repository: Option<String>,
) -> Result<()> {
    let path = context.root().join(path).canonicalize()?;
    let document = fs::read_to_string(&path)?;
    let options = Options {
        since,
        scope,
        exclude,
        repository,
    };
    let generated = generate(context, Some(&version), "HEAD", options)?.require_commits()?;
    let updated = insert_notes(&document, &version, &generated.notes)?;
    let directory = path.parent().unwrap_or(path.as_path());
    update_file_with(directory, &path, &updated, &mut SystemUpdateIo)
}

/// List the commits that the next release's notes would describe, newest first.
/// An empty list means there is nothing to release.
#[bake::task]
pub fn pending(
    context: &mut Context,
    #[bake(help = "Revision of the previous release; by default, the newest release tag.")]
    since: Option<String>,
    #[bake(help = "Git pathspec of the commits that count; by default, the whole repository.")]
    scope: Vec<String>,
    #[bake(help = "Path, relative to the repository root, whose commits do not count.")]
    exclude: Vec<String>,
) -> Result<Value> {
    let options = Options {
        since,
        scope,
        exclude,
        repository: None,
    };
    let generated = generate(context, None, "HEAD", options)?;
    let commits: Vec<BTreeMap<&str, &str>> = generated
        .commits
        .iter()
        .map(|commit| BTreeMap::from([("sha", commit.sha.as_str()), ("subject", &commit.subject)]))
        .collect();
    bake::value(commits)
}

fn first_difference(expected: &str, committed: &str) -> String {
    let (mut expected, mut committed) = (expected.lines(), committed.lines());
    let mut line = 1;
    loop {
        match (expected.next(), committed.next()) {
            (left, right) if left == right => line += 1,
            (left, right) => {
                return format!(
                    "line {line} should be {:?} but is {:?}",
                    left.unwrap_or_default(),
                    right.unwrap_or_default()
                );
            }
        }
    }
}

/// Check that the `## VERSION` section committed in `sha` is the one generated
/// from the history of its first parent, which for a merge candidate is the
/// target branch.
#[bake::task]
#[allow(clippy::too_many_arguments)]
pub fn verify(
    context: &mut Context,
    version: String,
    #[bake(default = "HEAD", help = "The release commit or merge candidate.")] sha: String,
    #[bake(default = "releases.md")] path: PathBuf,
    #[bake(help = "Revision of the previous release; by default, the newest earlier release tag.")]
    since: Option<String>,
    #[bake(help = "Git pathspec of the commits that count; by default, the whole repository.")]
    scope: Vec<String>,
    #[bake(help = "Path, relative to the repository root, whose commits do not count.")]
    exclude: Vec<String>,
    #[bake(help = "Repository URL for links; by default, the manifest's `repository`.")]
    repository: Option<String>,
) -> Result<Value> {
    let path = path.to_string_lossy();
    let sha = history::resolve(context, &sha)?;
    let parent = history::resolve(context, &format!("{sha}^1"))?;
    let options = Options {
        since,
        scope,
        exclude,
        repository,
    };
    let generated = generate(context, Some(&version), &parent, options)?.require_commits()?;
    let regenerated = insert_notes(
        &history::file_at(context, &parent, &path)?,
        &version,
        &generated.notes,
    )?;
    let committed = history::file_at(context, &sha, &path)?;
    let expected = extract_notes(&regenerated, &version)
        .unwrap_or_default()
        .trim();
    let committed = extract_notes(&committed, &version)?.trim();
    if !expected.lines().eq(committed.lines()) {
        return Err(Error::new(format!(
            "the {version} section of {path} differs from the notes generated from the history of {parent}: {}; prepare the release again from the current branch",
            first_difference(expected, committed)
        )));
    }
    bake::value(BTreeMap::from([
        ("version", Value::from(version)),
        ("commits", Value::from(generated.commits.len())),
    ]))
}

/// Fail when `head` changes the release document since its merge base with
/// `base` and any commit making the change, merges aside, is not a release
/// commit, since release notes are generated from Git history. `head` may be
/// GitHub's pull request or merge queue merge commit.
#[bake::task]
pub fn check(
    context: &mut Context,
    #[bake(help = "The pull request's base revision; empty or all zeros checks nothing.")]
    base: String,
    #[bake(default = "HEAD")] head: String,
    #[bake(default = "releases.md")] path: PathBuf,
) -> Result<Value> {
    if base.is_empty() || base.bytes().all(|byte| byte == b'0') {
        return bake::value(BTreeMap::from([("changed", false), ("release", false)]));
    }
    let path = path.to_string_lossy();
    let base = history::resolve(context, &base)?;
    let head = history::resolve(context, &head)?;
    let editors = history::editors(context, &base, &head, &path)?;
    let changed = history::changes(context, &base, &head, &path)?;
    let edits: Vec<&str> = editors
        .iter()
        .filter(|(_, release)| !release)
        .map(|(sha, _)| sha.get(..12).unwrap_or(sha))
        .collect();
    let release = changed && edits.is_empty() && !editors.is_empty();
    if changed && !release {
        let commits = if edits.is_empty() {
            "a merge".to_owned()
        } else {
            edits.join(", ")
        };
        return Err(Error::new(format!(
            "{path} is generated from Git history when a release is prepared, so pull requests do not edit it, but {commits} does: revert the change, write the release note as the pull request title, and describe any breaking change in a `Breaking:` paragraph of the description"
        )));
    }
    bake::value(BTreeMap::from([("changed", changed), ("release", release)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_file_reports_a_missing_destination_directory() {
        let directory = tempfile::tempdir().unwrap();
        let temporary = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        let destination = directory.path().join("missing/releases.md");

        assert!(persist_file(temporary, &destination).is_err());
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum UpdateOperation {
        CreateTemporary,
        Write,
        Permissions,
        SetPermissions,
        Sync,
        Persist,
    }

    struct FailingUpdateIo {
        fail_at: Option<UpdateOperation>,
        system: SystemUpdateIo,
    }

    impl FailingUpdateIo {
        fn new(fail_at: Option<UpdateOperation>) -> Self {
            Self {
                fail_at,
                system: SystemUpdateIo,
            }
        }

        fn fail_if(&self, operation: UpdateOperation) -> io::Result<()> {
            if self.fail_at == Some(operation) {
                Err(io::Error::other("simulated update failure"))
            } else {
                Ok(())
            }
        }
    }

    impl UpdateIo for FailingUpdateIo {
        fn create_temporary(&mut self, directory: &Path) -> io::Result<tempfile::NamedTempFile> {
            self.fail_if(UpdateOperation::CreateTemporary)?;
            self.system.create_temporary(directory)
        }

        fn write(&mut self, file: &mut File, contents: &[u8]) -> io::Result<()> {
            self.fail_if(UpdateOperation::Write)?;
            self.system.write(file, contents)
        }

        fn permissions(&mut self, path: &Path) -> io::Result<Permissions> {
            self.fail_if(UpdateOperation::Permissions)?;
            self.system.permissions(path)
        }

        fn set_permissions(&mut self, file: &File, permissions: Permissions) -> io::Result<()> {
            self.fail_if(UpdateOperation::SetPermissions)?;
            self.system.set_permissions(file, permissions)
        }

        fn sync_all(&mut self, file: &File) -> io::Result<()> {
            self.fail_if(UpdateOperation::Sync)?;
            self.system.sync_all(file)
        }

        fn persist(&mut self, temporary: tempfile::NamedTempFile, path: &Path) -> Result<()> {
            self.fail_if(UpdateOperation::Persist)?;
            self.system.persist(temporary, path)
        }
    }

    #[test]
    fn update_file_failures_preserve_the_original_document() {
        for operation in [
            UpdateOperation::CreateTemporary,
            UpdateOperation::Write,
            UpdateOperation::Permissions,
            UpdateOperation::SetPermissions,
            UpdateOperation::Sync,
            UpdateOperation::Persist,
        ] {
            let directory = tempfile::tempdir().unwrap();
            let destination = directory.path().join("releases.md");
            fs::write(&destination, "## Unreleased\n").unwrap();
            let mut operations = FailingUpdateIo::new(Some(operation));

            assert!(
                update_file_with(directory.path(), &destination, "## v1\n", &mut operations)
                    .is_err()
            );
            assert_eq!(fs::read_to_string(destination).unwrap(), "## Unreleased\n");
        }
    }

    #[test]
    fn update_file_succeeds_when_all_operations_succeed() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("releases.md");
        fs::write(&destination, "## Unreleased\n").unwrap();
        let mut operations = FailingUpdateIo::new(None);

        update_file_with(directory.path(), &destination, "## v1\n", &mut operations).unwrap();

        assert_eq!(fs::read_to_string(destination).unwrap(), "## v1\n");
    }
}

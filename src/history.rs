// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

//! The Git queries behind generated release notes. Every query names its
//! revisions explicitly and disables configuration that changes `git log`
//! output, so the same history always gives the same notes.

use crate::document::Commit;
use bake::{Context, Error, Result};
use std::cmp::Ordering;

/// Paths that hold only CI, agent and task tooling. A commit that touches only
/// these appears in no release notes.
pub(crate) const MAINTENANCE_PATHS: [&str; 3] = [".github", ".agents", "bake"];

const FIELD: char = '\x1f';
const RECORD: char = '\x1e';

/// Run `program`, failing unless it exits with one of `accepted` codes, and
/// return the code with standard output.
fn run(
    context: &Context,
    program: &str,
    arguments: &[&str],
    accepted: &[i32],
) -> Result<(i32, String)> {
    let output = context
        .command(program)
        .args([
            "-c",
            "log.showSignature=false",
            "-c",
            "core.quotePath=false",
        ])
        .args(arguments)
        .output()
        .map_err(|error| Error::new(format!("could not run {program}: {error}")))?;
    match output.status.code() {
        Some(code) if accepted.contains(&code) => {
            Ok((code, String::from_utf8_lossy(&output.stdout).into_owned()))
        }
        _ => Err(Error::new(format!(
            "{program} {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

fn git_text(context: &Context, arguments: &[&str]) -> Result<String> {
    run(context, "git", arguments, &[0]).map(|(_, output)| output)
}

/// Resolve a revision to a full commit SHA.
pub(crate) fn resolve(context: &Context, revision: &str) -> Result<String> {
    let commit = format!("{revision}^{{commit}}");
    Ok(git_text(
        context,
        &["rev-parse", "--verify", "--end-of-options", &commit],
    )?
    .trim()
    .to_owned())
}

/// Read a file, relative to the project root, as it is in `revision`.
pub(crate) fn file_at(context: &Context, revision: &str, path: &str) -> Result<String> {
    git_text(context, &["show", &format!("{revision}:./{path}")])
}

/// Whether `revision` carries a `Release:` trailer.
pub(crate) fn is_release(context: &Context, revision: &str) -> Result<bool> {
    let commit = format!("{revision}^{{commit}}");
    let trailers = git_text(
        context,
        &[
            "show",
            "--no-patch",
            "--format=format:%(trailers:key=Release,valueonly)",
            "--end-of-options",
            &commit,
            "--",
        ],
    )?;
    Ok(!trailers.trim().is_empty())
}

/// Whether `head` changes `path` relative to its merge base with `base`.
pub(crate) fn changes(context: &Context, base: &str, head: &str, path: &str) -> Result<bool> {
    let (base, head) = (resolve(context, base)?, resolve(context, head)?);
    let range = format!("{base}...{head}");
    let path = format!("./{path}");
    let arguments = ["diff", "--quiet", "--no-ext-diff", &range, "--", &path];
    let (code, _) = run(context, "git", &arguments, &[0, 1])?;
    Ok(code == 1)
}

/// The commits on the first-parent history of `until`, newest first, that are
/// not reachable from `since` and touch `scope`. Release commits are skipped.
pub(crate) fn commits(
    context: &Context,
    since: Option<&str>,
    until: &str,
    scope: &[String],
) -> Result<Vec<Commit>> {
    let until = resolve(context, until)?;
    let range = match since {
        Some(since) => format!("{}..{until}", resolve(context, since)?),
        None => until,
    };
    let format = "--format=format:%H%x1f%s%x1f%(trailers:key=Release,valueonly)%x1f%b%x1e";
    let mut arguments = vec![
        "log",
        "--first-parent",
        "--no-color",
        "--encoding=UTF-8",
        format,
        "--end-of-options",
        &range,
        "--",
    ];
    arguments.extend(scope.iter().map(String::as_str));
    let output = git_text(context, &arguments)?;
    Ok(output
        .split(RECORD)
        .map(|record| record.trim_start_matches('\n'))
        .filter(|record| !record.is_empty())
        .filter_map(|record| {
            let mut fields = record.splitn(4, FIELD);
            let mut field = || fields.next().unwrap_or_default();
            let (sha, subject, release, body) = (field(), field(), field(), field());
            release.trim().is_empty().then(|| Commit {
                sha: sha.to_owned(),
                subject: subject.to_owned(),
                body: body.trim_end().to_owned(),
            })
        })
        .collect())
}

/// The pathspec for a release: `scope` (the whole repository when empty)
/// without the maintenance paths and `excluded`. An excluded path is relative to
/// the repository root unless it is already a magic pathspec beginning with `:`.
pub(crate) fn pathspec(scope: &[String], excluded: &[String]) -> Vec<String> {
    let mut pathspec = if scope.is_empty() {
        vec![":(top)".to_owned()]
    } else {
        scope.to_vec()
    };
    for path in MAINTENANCE_PATHS
        .iter()
        .copied()
        .chain(excluded.iter().map(String::as_str))
    {
        pathspec.push(if path.starts_with(':') {
            path.to_owned()
        } else {
            format!(":(exclude,top){path}")
        });
    }
    pathspec
}

/// The newest tag beginning with `prefix`, whose remainder is a semantic
/// version below `below` (when given), that is reachable from `until`.
pub(crate) fn previous_tag(
    context: &Context,
    prefix: &str,
    until: &str,
    below: Option<&Version>,
) -> Result<Option<String>> {
    let pattern = format!("{prefix}*");
    let tags = git_text(context, &["tag", "--merged", until, "--list", &pattern])?;
    Ok(tags
        .lines()
        .filter_map(|tag| {
            let version = tag.strip_prefix(prefix).and_then(Version::parse)?;
            below
                .is_none_or(|below| version < *below)
                .then_some((version, tag))
        })
        .max()
        .map(|(_, tag)| tag.to_owned()))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    Numeric(u64),
    Text(String),
}

/// A semantic version, ordered by precedence; build metadata is ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Version {
    core: [u64; 3],
    prerelease: Vec<Identifier>,
}

impl Version {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let text = text.split_once('+').map_or(text, |(text, _)| text);
        let (core, prerelease) = match text.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (text, None),
        };
        let core: Vec<u64> = core.split('.').map(number).collect::<Option<_>>()?;
        let core: [u64; 3] = core.try_into().ok()?;
        let prerelease = match prerelease {
            None => Vec::new(),
            Some(prerelease) => prerelease
                .split('.')
                .map(|identifier| {
                    if identifier.is_empty()
                        || !identifier
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    {
                        None
                    } else {
                        Some(number(identifier).map_or_else(
                            || Identifier::Text(identifier.to_owned()),
                            Identifier::Numeric,
                        ))
                    }
                })
                .collect::<Option<_>>()?,
        };
        Some(Self { core, prerelease })
    }
}

fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core.cmp(&other.core).then_with(|| {
            match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.prerelease.cmp(&other.prerelease),
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn orders_versions_by_semantic_precedence() {
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.2.0",
            "10.0.0",
        ];
        for pair in ordered.windows(2) {
            assert!(version(pair[0]) < version(pair[1]), "{pair:?}");
            assert!(version(pair[1]) > version(pair[0]), "{pair:?}");
        }
        assert_eq!(version("1.0.0+build"), version("1.0.0"));
        assert_eq!(
            version("1.0.0").partial_cmp(&version("1.0.0")),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn rejects_versions_that_are_not_semantic() {
        for text in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "v1.2.3",
            "1.2.x",
            "1..3",
            "1.2.3-",
            "1.2.3-a..b",
            "1.2.3-a_b",
            "99999999999999999999.0.0",
        ] {
            assert!(Version::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn reports_revisions_that_do_not_resolve() {
        let directory = tempfile::tempdir().unwrap();
        let git = |arguments: &[&str]| {
            let status = std::process::Command::new("git")
                .args(arguments)
                .current_dir(directory.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "Fixture")
                .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
                .env("GIT_COMMITTER_NAME", "Fixture")
                .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "--quiet"]);
        git(&["commit", "--quiet", "--allow-empty", "--message", "Start."]);
        let registry = bake::Registry::discover().unwrap();
        let context = registry.context(directory.path());
        assert!(changes(&context, "HEAD", "missing", "releases.md").is_err());
        assert!(resolve(&context, "HEAD").unwrap().len() >= 40);
    }

    #[test]
    fn reports_programs_that_cannot_start() {
        let directory = tempfile::tempdir().unwrap();
        let registry = bake::Registry::discover().unwrap();
        let context = registry.context(directory.path());
        let error = run(&context, "bake-releases-missing-program", &[], &[0]).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("could not run bake-releases-missing-program")
        );
    }

    #[test]
    fn excludes_maintenance_and_configured_paths() {
        assert_eq!(
            pathspec(
                &[],
                &["benchmarks".to_owned(), ":(exclude)*.svg".to_owned()]
            ),
            [
                ":(top)",
                ":(exclude,top).github",
                ":(exclude,top).agents",
                ":(exclude,top)bake",
                ":(exclude,top)benchmarks",
                ":(exclude)*.svg",
            ]
        );
        assert_eq!(pathspec(&["crates/a".to_owned()], &[])[0], "crates/a");
    }
}

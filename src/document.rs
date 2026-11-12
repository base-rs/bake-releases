// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use base_markdown::{
    Document, NodeId, ParseOptions,
    stream::{EventKind, Tag, Text},
};
use std::ops::Range;

struct Heading {
    level: u8,
    title: String,
    title_range: Option<Range<usize>>,
    start: usize,
    body_start: usize,
}

fn plain_title_range(document: &Document<&str>, id: NodeId) -> Option<Range<usize>> {
    let mut children = document.children(id);
    let child = children.next()?;
    if children.next().is_some() {
        return None;
    }
    let EventKind::Text(Text::Source(range)) = &document.node(child).unwrap().kind else {
        return None;
    };
    let value = &document.source()[range.clone()];
    Some(range.start..range.start + chomp_line_ending(value).len())
}

fn heading_from_node(document: &Document<&str>, id: NodeId) -> Option<Heading> {
    let node = document.node(id).unwrap();
    let EventKind::Start(Tag::Heading(level)) = node.kind else {
        return None;
    };
    let source = document.source();
    let start = node.span.start;
    // Release section boundaries use only unindented ATX headings at document level.
    if (start != 0 && !matches!(source.as_bytes()[start - 1], b'\r' | b'\n'))
        || !source[start..].starts_with('#')
    {
        return None;
    }
    Some(Heading {
        level,
        title: chomp_line_ending(&document.text_content(id)).to_owned(),
        title_range: plain_title_range(document, id),
        start,
        body_start: node.span.end,
    })
}

fn chomp_line_ending(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix(['\r', '\n']))
        .unwrap_or(text)
}

// Restricting section boundaries to document children excludes headings in lists and quotes.
fn headings(source: &str) -> Vec<Heading> {
    let document = Document::parse(source, &ParseOptions::default())
        .expect("the built-in Markdown parser produces a valid document");
    document
        .children(document.root())
        .filter_map(|id| heading_from_node(&document, id))
        .collect()
}

fn unique_heading<'headings>(
    headings: &'headings [Heading],
    title: &str,
) -> Result<&'headings Heading> {
    let mut matching = headings.iter().filter(|heading| heading.title == title);
    let heading = matching
        .next()
        .ok_or_else(|| Error::new(format!("release heading {title:?} not found")))?;
    if matching.next().is_some() {
        return Err(Error::new(format!(
            "release heading {title:?} is ambiguous"
        )));
    }
    Ok(heading)
}
fn section_end(document: &str, headings: &[Heading], heading: &Heading) -> usize {
    headings
        .iter()
        .find(|candidate| candidate.start > heading.start && candidate.level <= heading.level)
        .map_or(document.len(), |candidate| candidate.start)
}

/// Extract the body under an exact ATX heading, retaining nested sections and
/// original Markdown bytes. Missing or duplicate headings are errors.
///
/// Headings must be unindented, e.g. `## v0.1.0`. Setext headings, HTML blocks,
/// and headings inside block quotes or lists are outside this document format.
pub fn extract_notes<'document>(document: &'document str, version: &str) -> Result<&'document str> {
    let headings = headings(document);
    let heading = unique_heading(&headings, version)?;
    let end = section_end(document, &headings, heading);
    Ok(&document[heading.body_start..end])
}

/// A commit in a release range: its full SHA, subject line and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// The full commit SHA.
    pub sha: String,
    /// The subject line, which is the pull request title for a squash merge.
    pub subject: String,
    /// The message body, which is the pull request description for a squash merge.
    pub body: String,
}

/// The release notes generated for one version: the breaking changes and one
/// entry for each commit, in the order of the commits given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    /// Each `Breaking:` paragraph, without the marker, followed by its link.
    pub breaking: Vec<String>,
    /// Each commit subject, with its trailing `(#N)` turned into a link.
    pub entries: Vec<String>,
}

impl Notes {
    /// Generate the notes for `commits`, newest first. Links follow GitHub's
    /// URL layout beneath `repository`; without one, subjects are kept verbatim.
    pub fn from_commits(commits: &[Commit], repository: Option<&str>) -> Self {
        let repository = repository
            .map(|url| url.trim().trim_end_matches('/'))
            .map(|url| url.strip_suffix(".git").unwrap_or(url))
            .filter(|url| !url.is_empty());
        let mut notes = Self::default();
        for commit in commits {
            let (text, link) = link(commit, repository);
            notes.entries.push(format!("{text}{link}"));
            for paragraph in breaking_paragraphs(&commit.body) {
                notes.breaking.push(format!("{paragraph}{link}"));
            }
        }
        notes
    }

    /// Render the section body from `kept`, the body of an existing
    /// `## Unreleased` section, and these notes: one **Breaking changes** list,
    /// then a `**Changes**` lead-in, so the two lists stay apart, then the
    /// changes. Each list holds what `kept` has for it, then the generated items.
    ///
    /// Content of `kept` after a `**Breaking changes**` lead-in, up to a
    /// `**Changes**` one, is kept as breaking changes, where a lead-in is a
    /// document-level heading or wholly strong paragraph of either title, in any
    /// case and with an optional trailing colon; the rest of it is kept as
    /// changes, without the lead-ins, so a kept section and the generated breaking
    /// changes share one list.
    ///
    /// The result is normalized as the `markdown:normalize` task normalizes
    /// documents, so normalizing a release document leaves it unchanged. Lines
    /// end with `\n`, and an empty result means there is nothing to write.
    pub fn render(&self, kept: &str) -> String {
        let kept = kept.replace("\r\n", "\n").replace('\r', "\n");
        let (kept_breaking, kept_changes) = split_kept(&kept);
        // A single line break continues a kept list rather than starting a loose one.
        let join = |kept: &str, items: &[String]| {
            [kept.to_owned(), list(items)]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        };
        let breaking = join(&kept_breaking, &self.breaking);
        let changes = join(&kept_changes, &self.entries);
        let mut blocks = Vec::new();
        if !breaking.is_empty() {
            blocks.push(format!("{BREAKING_LEAD_IN}\n\n{breaking}"));
            if !changes.is_empty() {
                blocks.push(CHANGES_LEAD_IN.to_owned());
            }
        }
        if !changes.is_empty() {
            blocks.push(changes);
        }
        normalize(&blocks.join("\n\n"))
    }
}

/// Normalize Markdown as the `markdown:normalize` task does, keeping text that
/// cannot be serialized as it is.
pub(crate) fn normalize(markdown: &str) -> String {
    base_markdown::normalize_markdown(markdown).unwrap_or(markdown.to_owned())
}

const BREAKING_LEAD_IN: &str = "**Breaking changes**";
const CHANGES_LEAD_IN: &str = "**Changes**";

/// Split a kept `## Unreleased` body into its breaking changes and its other
/// changes, each trimmed, at its document-level lead-in paragraphs.
fn split_kept(kept: &str) -> (String, String) {
    let document = Document::parse(kept, &ParseOptions::default())
        .expect("the built-in Markdown parser produces a valid document");
    let (mut breaking, mut changes) = (Vec::new(), Vec::new());
    let (mut start, mut in_breaking) = (0, false);
    let mut take = |range: Range<usize>, in_breaking: bool| {
        let part = kept[range].trim();
        if !part.is_empty() {
            let parts = if in_breaking {
                &mut breaking
            } else {
                &mut changes
            };
            parts.push(part);
        }
    };
    for id in document.children(document.root()) {
        let span = document.node(id).unwrap().span.clone();
        let Some(lead_in) = lead_in(&kept[span.clone()]) else {
            continue;
        };
        take(start..span.start, in_breaking);
        (start, in_breaking) = (span.end, lead_in);
    }
    take(start..kept.len(), in_breaking);
    (breaking.join("\n\n"), changes.join("\n\n"))
}

/// Whether a document-level block of a kept body is a breaking changes lead-in,
/// `Some(true)`, a changes one, `Some(false)`, or neither: a heading or a wholly
/// strong paragraph saying `Breaking changes` or `Changes`, in any case and with
/// an optional trailing colon, as `**Breaking changes:**` or `### Changes`.
fn lead_in(block: &str) -> Option<bool> {
    let block = normalize(block);
    let block = block.trim().trim_end_matches(':');
    let title = match block
        .strip_prefix("**")
        .and_then(|block| block.strip_suffix("**"))
    {
        Some(title) => title,
        None => {
            let title = block.trim_start_matches('#');
            if !(1..=6).contains(&(block.len() - title.len())) {
                return None;
            }
            title.strip_prefix(' ')?.trim_end_matches('#')
        }
    };
    match title.trim().trim_end_matches(':').to_lowercase().as_str() {
        "breaking changes" => Some(true),
        "changes" => Some(false),
        _ => None,
    }
}

fn list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split a trailing GitHub squash reference, ` (#N)`, from a subject.
fn pull_request(subject: &str) -> Option<(&str, &str)> {
    let (text, number) = subject.strip_suffix(')')?.rsplit_once(" (#")?;
    (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
        .then_some((text, number))
}

/// The subject text and the link suffix shared by its entry and breaking changes.
fn link<'commit>(commit: &'commit Commit, repository: Option<&str>) -> (&'commit str, String) {
    match (pull_request(&commit.subject), repository) {
        (Some((text, number)), Some(url)) => (text, format!(" ([#{number}]({url}/pull/{number}))")),
        (Some((text, number)), None) => (text, format!(" (#{number})")),
        (None, Some(url)) => {
            let short = commit.sha.get(..7).unwrap_or(&commit.sha);
            (
                commit.subject.as_str(),
                format!(" ([{short}]({url}/commit/{}))", commit.sha),
            )
        }
        (None, None) => (commit.subject.as_str(), String::new()),
    }
}

const MARKER: &str = "Breaking:";

/// Every body paragraph that begins `Breaking:`, in any case, its lines joined
/// with spaces and the marker removed.
fn breaking_paragraphs(body: &str) -> Vec<String> {
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    let mut paragraphs = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for line in body.split('\n').chain([""]) {
        let line = line.trim();
        if !line.is_empty() {
            lines.push(line);
            continue;
        }
        if let Some(first) = lines.first()
            && let Some(marker) = first.get(..MARKER.len())
            && marker.eq_ignore_ascii_case(MARKER)
        {
            let rest = &first[MARKER.len()..];
            let text = std::iter::once(rest.trim())
                .chain(lines[1..].iter().copied())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if !text.is_empty() {
                paragraphs.push(text);
            }
        }
        lines.clear();
    }
    paragraphs
}

/// Render the section body for `commits`, as `releases:update` writes it when
/// the document has no `## Unreleased` section.
pub fn render_section(commits: &[Commit], repository: Option<&str>) -> String {
    Notes::from_commits(commits, repository).render("")
}

fn line_ending(document: &str) -> &'static str {
    if document.contains("\r\n") {
        "\r\n"
    } else if document.contains('\r') {
        "\r"
    } else {
        "\n"
    }
}

/// Check that `version` is a valid new heading in a document with `headings`.
fn validate_version(headings: &[Heading], version: &str) -> Result<()> {
    if version.is_empty()
        || version.trim() != version
        || version.chars().any(char::is_control)
        || version.contains('#')
        || version == "Unreleased"
    {
        return Err(Error::new(
            "version must be a nonempty single-line heading other than Unreleased, without # characters",
        ));
    }
    if headings.iter().any(|heading| heading.title == version) {
        return Err(Error::new(format!(
            "release heading {version:?} already exists"
        )));
    }
    Ok(())
}

/// Replace exactly one `Unreleased` heading without reformatting the document.
/// Existing version headings and multiline version strings are rejected.
#[deprecated(
    note = "release notes are generated from Git history; use `insert_notes`, which keeps an existing `## Unreleased` body"
)]
pub fn update_document(document: &str, version: &str) -> Result<String> {
    let headings = headings(document);
    validate_version(&headings, version)?;
    let heading = unique_heading(&headings, "Unreleased")?;
    let title_range = heading
        .title_range
        .as_ref()
        .ok_or_else(|| Error::new("Unreleased heading must have a plain text title"))?;
    let mut output = document.to_owned();
    output.replace_range(title_range.clone(), version);
    Ok(output)
}

/// Insert a `## {version}` section holding `notes` before the first `##`
/// heading, or at the end, preserving every other byte of the document.
///
/// An existing `## Unreleased` section is replaced: its body is kept, as
/// [`Notes::render`] describes, so hand-written notes from before generated
/// notes are not lost. Existing version headings, duplicate
/// `Unreleased` sections, empty notes and multiline versions are rejected.
pub fn insert_notes(document: &str, version: &str, notes: &Notes) -> Result<String> {
    let headings = headings(document);
    validate_version(&headings, version)?;
    let mut unreleased = headings
        .iter()
        .filter(|heading| heading.title == "Unreleased");
    let (start, end, kept) = match (unreleased.next(), unreleased.next()) {
        (Some(_), Some(_)) => {
            return Err(Error::new("release heading \"Unreleased\" is ambiguous"));
        }
        (Some(heading), None) => {
            let end = section_end(document, &headings, heading);
            (heading.start, end, &document[heading.body_start..end])
        }
        (None, _) => {
            let start = headings
                .iter()
                .find(|heading| heading.level == 2)
                .map_or(document.len(), |heading| heading.start);
            (start, start, "")
        }
    };
    let body = notes.render(kept);
    if body.is_empty() {
        return Err(Error::new(format!(
            "there are no release notes for {version}"
        )));
    }

    let ending = line_ending(document);
    let (before, after) = (&document[..start], &document[end..]);
    let blank = format!("{ending}{ending}");
    let mut output = before.to_owned();
    if !before.is_empty() && !before.ends_with(&blank) {
        output.push_str(if before.ends_with(ending) {
            ending
        } else {
            &blank
        });
    }
    output.push_str(&format!("## {version}{ending}{ending}"));
    output.push_str(&body.replace('\n', ending));
    if !after.is_empty() {
        output.push_str(ending);
    }
    output.push_str(after);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str, subject: &str, body: &str) -> Commit {
        Commit {
            sha: sha.to_owned(),
            subject: subject.to_owned(),
            body: body.to_owned(),
        }
    }

    fn notes(entries: &[&str]) -> Notes {
        Notes {
            breaking: Vec::new(),
            entries: entries.iter().map(|entry| (*entry).to_owned()).collect(),
        }
    }

    #[test]
    fn preserves_source_offsets_for_every_line_ending() {
        for ending in ["\n", "\r", "\r\n"] {
            let source = "# Releases\n\n## Unreleased\n\nNotes.\n\n## v0.1.0\n\nOld.\n"
                .replace('\n', ending);
            let parsed = headings(&source);
            let heading = &parsed[1];
            assert_eq!(
                &source[heading.start..heading.body_start],
                format!("## Unreleased{ending}")
            );
            assert_eq!(
                extract_notes(&source, "Unreleased").unwrap(),
                format!("{ending}Notes.{ending}{ending}")
            );
            let updated = insert_notes(&source, "v0.2.0", &notes(&["New."])).unwrap();
            assert_eq!(
                updated,
                "# Releases\n\n## v0.2.0\n\nNotes.\n\n- New.\n\n## v0.1.0\n\nOld.\n"
                    .replace('\n', ending)
            );
        }
    }

    #[test]
    #[allow(deprecated)]
    fn renames_unreleased_with_plain_title_ranges_for_every_line_ending() {
        for ending in ["\n", "\r", "\r\n"] {
            let source = "# Releases\n\n## Unreleased\n\nNotes.\n".replace('\n', ending);
            let heading = &headings(&source)[1];
            assert_eq!(&source[heading.title_range.clone().unwrap()], "Unreleased");
            let updated = update_document(&source, "v0.2.0").unwrap();
            assert_eq!(updated, source.replacen("Unreleased", "v0.2.0", 1));
        }
        assert!(headings("##\n").first().unwrap().title_range.is_none());
        for title in ["**Unreleased**", "`Unreleased`", "[Unreleased](/url)"] {
            let source = format!("## {title}\n\nNotes.\n");
            let error = update_document(&source, "v0.2.0").unwrap_err();
            assert!(error.to_string().contains("plain text title"));
        }
        assert!(update_document("## v0\n", "v1").is_err());
        assert!(update_document("## Unreleased\n", "v1#").is_err());
    }

    #[test]
    fn empty_headings_have_an_empty_title() {
        let parsed = headings("##\n\nBody.\n");
        assert_eq!(parsed[0].title, "");
        assert_eq!(extract_notes("##\n\nBody.\n", "").unwrap(), "\nBody.\n");
    }

    #[test]
    fn preserves_single_line_atx_section_rules() {
        let source = "# Releases\n\n  ## Indented\n\nSetext\n------\n\n> ## Quoted\n\n- ## Listed\n\n```\n## Fenced\n```\n\n## Real\n";
        let parsed = headings(source);
        assert_eq!(
            parsed
                .iter()
                .map(|heading| heading.title.as_str())
                .collect::<Vec<_>>(),
            ["Releases", "Real"]
        );
    }

    #[test]
    fn formatted_unreleased_titles_are_migrated_too() {
        for title in [
            "**Unreleased**",
            "Unreleas&#101;d",
            "`Unreleased`",
            "[Unreleased](/url)",
        ] {
            let source = format!("## {title}\n\nNotes.\n");
            assert_eq!(extract_notes(&source, "Unreleased").unwrap(), "\nNotes.\n");
            assert_eq!(
                insert_notes(&source, "v0.2.0", &notes(&["New."])).unwrap(),
                "## v0.2.0\n\nNotes.\n\n- New.\n"
            );
        }
    }

    #[test]
    fn recognizes_only_trailing_numeric_pull_request_references() {
        assert_eq!(pull_request("Fix it. (#12)"), Some(("Fix it.", "12")));
        assert_eq!(pull_request("Fix it. (#)"), None);
        assert_eq!(pull_request("Fix it. (#1a)"), None);
        assert_eq!(pull_request("Fix (#12) it."), None);
        assert_eq!(pull_request("Fix it.(#12)"), None);
    }

    #[test]
    fn links_pull_requests_and_commits_beneath_the_repository() {
        let commits = [
            commit("0123456789abcdef", "Direct push.", ""),
            commit("fedcba9876543210", "Squashed. (#7)", ""),
            commit("abc", "Short SHA.", ""),
        ];
        for repository in [
            "https://github.com/base-rs/example",
            "https://github.com/base-rs/example/",
            "https://github.com/base-rs/example.git",
        ] {
            assert_eq!(
                Notes::from_commits(&commits, Some(repository)).entries,
                [
                    "Direct push. ([0123456](https://github.com/base-rs/example/commit/0123456789abcdef))",
                    "Squashed. ([#7](https://github.com/base-rs/example/pull/7))",
                    "Short SHA. ([abc](https://github.com/base-rs/example/commit/abc))",
                ]
            );
        }
        for repository in [None, Some(""), Some("  ")] {
            assert_eq!(
                Notes::from_commits(&commits, repository).entries,
                ["Direct push.", "Squashed. (#7)", "Short SHA."]
            );
        }
    }

    #[test]
    fn extracts_every_breaking_paragraph_with_its_link() {
        let body = "Intro.\r\n\r\nBreaking: `run` takes a\r\n  `Context`.\r\n\r\nBreaking:\nRename `a`.\n   \nNot Breaking: here.\n\nBreakingé is no marker.\n\nBREAKING: Remove `c`.\n\nBreaking:\n\nCo-authored-by: Someone <someone@example.com>";
        let commits = [
            commit("1111111111", "Break things. (#3)", body),
            commit("2222222222", "Direct.", "breaking: Drop `b`."),
        ];
        let generated = Notes::from_commits(&commits, None);
        assert_eq!(
            generated.breaking,
            [
                "`run` takes a `Context`. (#3)",
                "Rename `a`. (#3)",
                "Remove `c`. (#3)",
                "Drop `b`.",
            ]
        );
        assert_eq!(
            generated.render("\n\n- Kept.\r\n\r\n"),
            "**Breaking changes**\n\n- `run` takes a `Context`. (#3)\n- Rename `a`. (#3)\n- Remove `c`. (#3)\n- Drop `b`.\n\n**Changes**\n\n- Kept.\n- Break things. (#3)\n- Direct.\n"
        );
        let breaking_only = Notes {
            breaking: vec!["Drop `c`.".to_owned()],
            entries: Vec::new(),
        };
        assert_eq!(
            breaking_only.render(""),
            "**Breaking changes**\n\n- Drop `c`.\n"
        );
    }

    #[test]
    fn merges_kept_and_generated_breaking_changes_into_one_list() {
        let generated = Notes {
            breaking: vec!["New break. (#9)".to_owned()],
            entries: vec!["New. (#9)".to_owned()],
        };
        let kept = "\r\n**Breaking changes**\r\n\r\n- Old break. (#1)\r\n  - Detail.\r\n\r\n**Changes**\r\n\r\n- Old. (#1)\r\n";
        let merged = "**Breaking changes**\n\n- Old break. (#1)\n  - Detail.\n- New break. (#9)\n\n**Changes**\n\n- Old. (#1)\n- New. (#9)\n";
        assert_eq!(generated.render(kept), merged);
        // Rendering its own output again adds nothing.
        assert_eq!(Notes::default().render(merged), merged);
        // Kept text before a lead-in is a change, and a lone lead-in is dropped.
        assert_eq!(
            generated.render("Intro.\n\n**Changes**\n\n- Old.\n"),
            "**Breaking changes**\n\n- New break. (#9)\n\n**Changes**\n\nIntro.\n\n- Old.\n- New. (#9)\n"
        );
        assert_eq!(notes(&["New."]).render("**Changes**\n"), "- New.\n");
        assert_eq!(Notes::default().render("**Breaking changes**\n"), "");
        // Headings, case, a colon and `__` spell lead-ins too, but prose does not.
        for spelling in [
            "### Breaking changes\n\n- Old break.\n\n#### Changes ####\n\n- Old.\n",
            "**Breaking Changes:**\n\n- Old break.\n\n__Changes__:\n\n- Old.\n",
        ] {
            assert_eq!(
                generated.render(spelling),
                "**Breaking changes**\n\n- Old break.\n- New break. (#9)\n\n**Changes**\n\n- Old.\n- New. (#9)\n",
                "{spelling:?}"
            );
        }
        for prose in [
            "**Breaking changes** follow.\n",
            "**Breaking** changes\n",
            "Changes\n",
            "####### Changes\n",
            "#Changes\n",
            "**Old** and **Changes**\n",
        ] {
            assert_eq!(
                Notes::default().render(prose),
                normalize(prose),
                "{prose:?}"
            );
        }
        // Lead-ins inside lists, quotes and code are content, not boundaries.
        let nested = "- **Changes**\n\n> **Changes**\n\n```\n**Changes**\n```\n";
        assert_eq!(
            Notes::default().render(&format!("**Breaking changes**\n\n{nested}")),
            format!("**Breaking changes**\n\n{nested}")
        );
    }

    #[test]
    fn renders_in_the_normalized_form_of_release_documents() {
        let subject = "Multiply 2 * 3 for `a_b`. (#5)";
        let rendered = render_section(&[commit("1", subject, "")], None);
        assert_eq!(rendered, "- Multiply 2 \\* 3 for `a_b`. (#5)\n");
        assert_eq!(normalize(&rendered), rendered);
    }

    #[test]
    fn renders_nothing_without_notes() {
        assert_eq!(render_section(&[], None), "");
        assert_eq!(Notes::default().render(" \n "), "");
        assert_eq!(render_section(&[commit("1", "One.", "")], None), "- One.\n");
    }
}

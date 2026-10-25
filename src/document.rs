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

/// Extract the body under an exact ATX heading, retaining nested sections and
/// original Markdown bytes. Missing or duplicate headings are errors.
///
/// Headings must be unindented, e.g. `## v0.1.0`. Setext headings, HTML blocks,
/// and headings inside block quotes or lists are outside this document format.
pub fn extract_notes<'document>(document: &'document str, version: &str) -> Result<&'document str> {
    let headings = headings(document);
    let heading = unique_heading(&headings, version)?;
    let end = headings
        .iter()
        .find(|candidate| candidate.start > heading.start && candidate.level <= heading.level)
        .map_or(document.len(), |candidate| candidate.start);
    Ok(&document[heading.body_start..end])
}

/// Replace exactly one `Unreleased` heading without reformatting the document.
/// Existing version headings and multiline version strings are rejected.
pub fn update_document(document: &str, version: &str) -> Result<String> {
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
    let headings = headings(document);
    if headings.iter().any(|heading| heading.title == version) {
        return Err(Error::new(format!(
            "release heading {version:?} already exists"
        )));
    }
    let heading = unique_heading(&headings, "Unreleased")?;
    let title_range = heading
        .title_range
        .as_ref()
        .ok_or_else(|| Error::new("Unreleased heading must have a plain text title"))?;
    let mut output = document.to_owned();
    output.replace_range(title_range.clone(), version);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_source_offsets_and_plain_title_ranges_for_every_line_ending() {
        for ending in ["\n", "\r", "\r\n"] {
            let source = "# Releases\n\n## Unreleased\n\nNotes.\n".replace('\n', ending);
            let parsed = headings(&source);
            let heading = &parsed[1];
            assert_eq!(&source[heading.title_range.clone().unwrap()], "Unreleased");
            assert_eq!(
                &source[heading.start..heading.body_start],
                format!("## Unreleased{ending}")
            );
            assert_eq!(
                extract_notes(&source, "Unreleased").unwrap(),
                format!("{ending}Notes.{ending}")
            );
            let updated = update_document(&source, "v0.2.0").unwrap();
            assert_eq!(updated, source.replacen("Unreleased", "v0.2.0", 1));
        }
    }

    #[test]
    fn empty_headings_have_no_plain_title_range() {
        let parsed = headings("##\n\nBody.\n");
        assert_eq!(parsed[0].title, "");
        assert!(parsed[0].title_range.is_none());
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
    fn extracts_formatted_titles_but_requires_plain_titles_for_replacement() {
        for title in [
            "**Unreleased**",
            "Unreleas&#101;d",
            "`Unreleased`",
            "[Unreleased](/url)",
        ] {
            let source = format!("## {title}\n\nNotes.\n");
            assert_eq!(extract_notes(&source, "Unreleased").unwrap(), "\nNotes.\n");
            assert!(
                update_document(&source, "v0.2.0")
                    .unwrap_err()
                    .to_string()
                    .contains("plain text title")
            );
        }
    }
}

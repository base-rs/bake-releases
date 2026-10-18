// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use core_markdown::{ParseOptions, mdast::Node, to_mdast};
use std::fmt::Display;
use std::ops::Range;

struct Heading {
    level: u8,
    title: String,
    title_range: Option<Range<usize>>,
    start: usize,
    body_start: usize,
}

fn after_heading_line(document: &str, offset: usize) -> usize {
    let rest = document.get(offset..).unwrap_or_default();

    if rest.starts_with("\r\n") {
        offset + 2
    } else if rest.starts_with('\r') || rest.starts_with('\n') {
        offset + 1
    } else {
        offset
    }
}

fn plain_title_range(
    document: &str,
    heading: &core_markdown::mdast::Heading,
) -> Option<Range<usize>> {
    let [Node::Text(text)] = heading.children.as_slice() else {
        return None;
    };
    let position = text.position.as_ref()?;
    let title = chomp_line_ending(&text.value);
    let mut end = position.end.offset;
    let prefix = document.get(..end)?;
    if prefix.ends_with("\r\n") {
        end -= 2;
    } else if prefix.ends_with('\r') || prefix.ends_with('\n') {
        end -= 1;
    }
    let range = position.start.offset..end;
    (document.get(range.clone())? == title).then_some(range)
}

fn heading_from_node(document: &str, node: &Node) -> Option<Heading> {
    let Node::Heading(heading) = node else {
        return None;
    };
    let position = heading.position.as_ref()?;
    let start = position.start.offset;

    // Replacements operate on single-line ATX release headings. The parser
    // has already distinguished headings from fences and other Markdown.
    if position.start.column != 1
        || !document
            .get(start..)
            .is_some_and(|rest| rest.starts_with('#'))
    {
        return None;
    }

    Some(Heading {
        level: heading.depth,
        title: chomp_line_ending(&node.text_content()).to_owned(),
        title_range: plain_title_range(document, heading),
        start,
        body_start: after_heading_line(document, position.end.offset),
    })
}

fn chomp_line_ending(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}

fn markdown_parse_error(error: impl Display) -> Error {
    Error::new(format!("could not parse release Markdown: {error}"))
}

fn missing_document_children_error() -> Error {
    Error::new("parsed release Markdown has no document children")
}

fn document_children(root: &Node) -> Result<&[Node]> {
    root.children()
        .map(Vec::as_slice)
        .ok_or_else(missing_document_children_error)
}

fn map_parse_result(result: std::result::Result<Node, impl Display>) -> Result<Node> {
    result.map_err(markdown_parse_error)
}

// Release documents use unindented ATX headings. Restricting section boundaries
// to document children excludes headings inside lists and block quotes.
fn headings(document: &str) -> Result<Vec<Heading>> {
    headings_with(document, |document| {
        to_mdast(document, &ParseOptions::default())
    })
}

fn headings_with<E: Display>(
    document: &str,
    parse: impl FnOnce(&str) -> std::result::Result<Node, E>,
) -> Result<Vec<Heading>> {
    let root = map_parse_result(parse(document))?;
    headings_from_root(document, &root)
}

fn headings_from_root(document: &str, root: &Node) -> Result<Vec<Heading>> {
    let children = document_children(root)?;
    let mut headings = Vec::new();

    for node in children {
        if let Some(heading) = heading_from_node(document, node) {
            headings.push(heading);
        }
    }

    Ok(headings)
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
    extract_notes_with(document, version, headings(document))
}

fn extract_notes_with<'document>(
    document: &'document str,
    version: &str,
    result: Result<Vec<Heading>>,
) -> Result<&'document str> {
    let headings = result?;
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
    update_document_with(document, version, headings)
}

fn update_document_with(
    document: &str,
    version: &str,
    get_headings: impl FnOnce(&str) -> Result<Vec<Heading>>,
) -> Result<String> {
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
    let headings = get_headings(document)?;
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
    use core_markdown::{
        mdast::{Heading as MarkdownHeading, Text},
        unist::Position,
    };

    #[test]
    fn heading_line_offsets_cover_line_endings_and_invalid_offsets() {
        assert_eq!(after_heading_line("text\r\n", 4), 6);
        assert_eq!(after_heading_line("text\r", 4), 5);
        assert_eq!(after_heading_line("text\n", 4), 5);
        assert_eq!(after_heading_line("text", 10), 10);
    }

    #[test]
    fn title_ranges_chomp_line_endings() {
        for ending in ["\r\n", "\r", "\n"] {
            let value = format!("title{ending}");
            let document = value.clone();
            let end_offset = value.len();
            let heading = MarkdownHeading {
                children: vec![Node::Text(Text {
                    value,
                    position: Some(Position::new(1, 1, 0, 2, 1, end_offset)),
                })],
                position: None,
                depth: 2,
            };

            assert_eq!(plain_title_range(&document, &heading), Some(0..5));
        }
    }

    #[test]
    fn title_ranges_reject_missing_positions_and_invalid_offsets() {
        let missing_position = MarkdownHeading {
            children: vec![Node::Text(Text {
                value: "title".into(),
                position: None,
            })],
            position: None,
            depth: 2,
        };
        assert!(plain_title_range("title", &missing_position).is_none());

        let invalid_end = MarkdownHeading {
            children: vec![Node::Text(Text {
                value: "title".into(),
                position: Some(Position::new(1, 1, 0, 2, 1, 99)),
            })],
            position: None,
            depth: 2,
        };
        assert!(plain_title_range("title", &invalid_end).is_none());

        let invalid_range = MarkdownHeading {
            children: vec![Node::Text(Text {
                value: "title".into(),
                position: Some(Position::new(1, 1, 99, 1, 6, 5)),
            })],
            position: None,
            depth: 2,
        };
        assert!(plain_title_range("title", &invalid_range).is_none());
    }

    #[test]
    fn headings_without_positions_are_ignored() {
        let node = Node::Heading(MarkdownHeading {
            children: vec![Node::Text(Text {
                value: "Unreleased".into(),
                position: None,
            })],
            position: None,
            depth: 2,
        });

        assert!(heading_from_node("## Unreleased\n", &node).is_none());
    }

    #[test]
    fn reports_markdown_parser_errors_and_non_document_roots() {
        assert!(headings_with("invalid Markdown", |_| Err("parser failed")).is_err());

        let error = markdown_parse_error("invalid Markdown");
        assert!(
            error
                .to_string()
                .contains("could not parse release Markdown")
        );

        assert!(
            document_children(&Node::Text(Text {
                value: "not a document".into(),
                position: None,
            }))
            .is_err()
        );
        assert!(
            headings_from_root(
                "not a document",
                &Node::Text(Text {
                    value: "not a document".into(),
                    position: None,
                })
            )
            .is_err()
        );
    }

    #[test]
    fn note_and_update_helpers_propagate_parser_errors() {
        let error = |_: &str| Err(Error::new("parser failed"));
        assert!(extract_notes_with("## v1\n", "v1", error("## v1\n")).is_err());
        assert!(update_document_with("## Unreleased\n", "v1", error).is_err());
    }
}

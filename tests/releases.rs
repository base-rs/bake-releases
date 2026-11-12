// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake_releases::{Notes, extract_notes, update_document};
use std::fs;

fn notes(entries: &[&str]) -> Notes {
    Notes {
        breaking: Vec::new(),
        entries: entries.iter().map(|entry| (*entry).to_owned()).collect(),
    }
}

#[test]
fn preserves_nested_sections_and_markdown() {
    let document =
        "# Releases\n\n## v1.0.0\n\nHello **world**.\n\n### Features\n\n- One\n\n## v0.9.0\nOld\n";
    assert_eq!(
        extract_notes(document, "v1.0.0").unwrap(),
        "\nHello **world**.\n\n### Features\n\n- One\n\n"
    );
    assert_eq!(extract_notes(document, "v0.9.0").unwrap(), "Old\n");
}

#[test]
fn ignores_headings_in_fenced_code() {
    for fence in ["```", "~~~~", "  ```rust"] {
        let closing = if fence.contains('~') { "~~~~" } else { "```" };
        let body = format!("\n{fence}\n## Unreleased\n## v0.1.0\n{closing}\n\nText\n");
        let document = format!("## v1.0.0\n{body}## v0.1.0\nOlder\n");
        assert_eq!(extract_notes(&document, "v1.0.0").unwrap(), body);
        assert_eq!(
            update_document(&document, "v2.0.0", &notes(&["New."])).unwrap(),
            format!("## v2.0.0\n\n- New.\n\n{document}")
        );
    }
}

#[test]
fn fences_require_a_sufficiently_long_matching_close() {
    let document = "## v1\n````\n```\n## v0\n~~~~\n## v0\n````\nDone\n## v0\nOld\n";
    assert!(extract_notes(document, "v1").unwrap().ends_with("Done\n"));
    assert_eq!(extract_notes(document, "v0").unwrap(), "Old\n");
}

#[test]
fn headings_are_exact_unique_and_unindented() {
    assert!(extract_notes("## v1.0.0\nHi\n", "1.0.0").is_err());
    assert!(extract_notes("## v1\nFirst\n## v1\nSecond\n", "v1").is_err());
    assert!(extract_notes("    ## v1\n", "v1").is_err());
    assert!(extract_notes("  ## v1\n", "v1").is_err());
    assert!(extract_notes("Title\n-----\n", "Title").is_err());
    assert!(extract_notes("##v1\n", "v1").is_err());
    assert_eq!(extract_notes("## v1 ###\nHi", "v1").unwrap(), "Hi");
    assert_eq!(extract_notes("## v1", "v1").unwrap(), "");
}

#[test]
fn inserts_the_section_before_the_first_release_heading() {
    let notes = notes(&["Two.", "One."]);
    for (document, expected) in [
        (
            "# Releases\n\n## v0\n\nOld.\n",
            "# Releases\n\n## v1\n\n- Two.\n- One.\n\n## v0\n\nOld.\n",
        ),
        (
            "# Releases\n## v0\n",
            "# Releases\n\n## v1\n\n- Two.\n- One.\n\n## v0\n",
        ),
        ("# Releases\n", "# Releases\n\n## v1\n\n- Two.\n- One.\n"),
        ("# Releases\n\n", "# Releases\n\n## v1\n\n- Two.\n- One.\n"),
        ("# Releases", "# Releases\n\n## v1\n\n- Two.\n- One.\n"),
        ("", "## v1\n\n- Two.\n- One.\n"),
        (
            "# Releases\n\n### Notes\n\n## v0\n",
            "# Releases\n\n### Notes\n\n## v1\n\n- Two.\n- One.\n\n## v0\n",
        ),
    ] {
        assert_eq!(update_document(document, "v1", &notes).unwrap(), expected);
    }
}

#[test]
fn keeps_the_unreleased_body_between_breaking_changes_and_entries() {
    let notes = Notes {
        breaking: vec!["Drop `a`.".to_owned()],
        entries: vec!["Generated.".to_owned()],
    };
    let document = "# Releases\n\n## Unreleased\n\n- Hand-written.\n\n### Detail\n\nMore.\n\n\n## v0\n\nOld.\n";
    assert_eq!(
        update_document(document, "v1", &notes).unwrap(),
        "# Releases\n\n## v1\n\n**Breaking changes**\n\n- Drop `a`.\n\n**Changes**\n\n- Hand-written.\n\n### Detail\n\nMore.\n\n- Generated.\n\n## v0\n\nOld.\n"
    );
    assert_eq!(
        update_document("## Unreleased\n\n- Only.\n", "v1", &Notes::default()).unwrap(),
        "## v1\n\n- Only.\n"
    );
}

#[test]
fn updates_preserve_crlf_and_unicode() {
    let document = "# Releases\r\n\r\n## Unreleased ##\r\n\r\nこんにちは\r\n\r\n## v0\r\nOld\r\n";
    assert_eq!(
        update_document(document, "v1", &notes(&["さようなら"])).unwrap(),
        "# Releases\r\n\r\n## v1\r\n\r\nこんにちは\r\n\r\n- さようなら\r\n\r\n## v0\r\nOld\r\n"
    );
}

#[test]
fn invalid_updates_leave_no_result() {
    let entries = notes(&["New."]);
    for version in ["", "Unreleased", " v1", "v1 ", "v1\n## Bad", "v1\0", "v1#"] {
        assert!(
            update_document("## Unreleased\n", version, &entries).is_err(),
            "{version:?}"
        );
    }
    assert!(update_document("## Unreleased\n## v1\n", "v1", &entries).is_err());
    assert!(
        update_document("## Unreleased\n## Unreleased\n", "v1", &entries)
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
    assert!(
        update_document("## Unreleased\n\n## v0\n", "v1", &Notes::default())
            .unwrap_err()
            .to_string()
            .contains("no release notes")
    );
}

#[test]
fn tasks_read_a_custom_document_and_report_missing_files() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("changes.md"),
        "# Releases\n\n## v1\n\nOld feature.\n",
    )
    .unwrap();
    fs::write(directory.path().join("invalid.md"), [0xff]).unwrap();
    let registry = bake::Registry::discover().unwrap();
    let mut context = registry.context(directory.path());
    assert_eq!(
        context
            .call("releases:notes", &["v1", "--path", "changes.md"])
            .unwrap(),
        "\nOld feature.\n"
    );
    for (task, arguments) in [
        ("releases:notes", ["v1", "--path", "missing.md"]),
        ("releases:notes", ["v2", "--path", "changes.md"]),
        ("releases:update", ["v2", "--path", "missing.md"]),
        ("releases:update", ["v2", "--path", "invalid.md"]),
    ] {
        assert!(context.call(task, &arguments).is_err(), "{arguments:?}");
    }
}

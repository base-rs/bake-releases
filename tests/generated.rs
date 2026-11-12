// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

//! Release notes generated from the history of a fixture repository.

use bake::{Registry, Result, Value};
use bake_releases as _;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const REPOSITORY: &str = "https://github.com/base-rs/fixture";

const MANIFEST: &str = r#"[package]
name = "fixture"
version = "0.1.0"
repository = "https://github.com/base-rs/fixture"

[package.metadata.bake.release]
notes-exclude = ["benchmarks"]
"#;

const RELEASES: &str = "# Releases\n\n## v0.1.0\n\n- First.\n";

struct Fixture {
    directory: TempDir,
}

impl Fixture {
    /// An empty repository whose commits have fixed authors and dates, so the
    /// same steps always produce the same SHAs.
    fn empty() -> Self {
        let fixture = Self {
            directory: tempfile::tempdir().unwrap(),
        };
        fixture.git(&["init", "--quiet", "--initial-branch=main"]);
        fixture
    }

    /// A released v0.1.0 followed by squash merges, a CI-only change, a
    /// release commit, an excluded benchmark, a direct push and a change that
    /// also touches the task package.
    fn new(releases: &str) -> Self {
        let fixture = Self::empty();
        fixture.commit(
            &[
                ("Cargo.toml", MANIFEST),
                ("releases.md", releases),
                ("src/lib.rs", "// Library.\n"),
            ],
            "Initial commit.",
        );
        fixture.git(&["tag", "v0.1.0"]);
        fixture.commit(&[("src/parser.rs", "// Parser.\n")], "Add the parser. (#2)");
        fixture.commit(
            &[(".github/workflows/test.yml", "name: Test\n")],
            "Update CI. (#3)",
        );
        fixture.commit(
            &[("src/parser.rs", "// Parser returning a result.\n")],
            "Change the parser's result. (#4)\n\nCallers now see errors.\r\n\r\nBreaking: `parse` returns a\r\n`Result`.\r\n\r\nCo-authored-by: Someone <someone@example.com>",
        );
        fixture.commit(
            &[("src/lib.rs", "// Library, released.\n")],
            "Release version 0.1.1.\n\nRelease: v0.1.1",
        );
        fixture.commit(
            &[("benchmarks/parse.rs", "// Benchmark.\n")],
            "Measure the parser. (#5)",
        );
        fixture.commit(&[("readme.md", "# Fixture\n")], "Fix a typo.");
        fixture.commit(
            &[
                ("bake/src/main.rs", "// Tasks.\n"),
                ("src/lib.rs", "// Library, shared.\n"),
            ],
            "Share the task package. (#6)",
        );
        fixture
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn git(&self, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(self.root())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn commit(&self, files: &[(&str, &str)], message: &str) -> String {
        for (path, contents) in files {
            self.write(path, contents);
        }
        self.git(&["add", "--all"]);
        self.git(&[
            "commit",
            "--quiet",
            "--allow-empty",
            "--cleanup=verbatim",
            "--message",
            message,
        ]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn sha(&self, subject: &str) -> String {
        self.git(&["log", "--format=%H", "--fixed-strings", "--grep", subject])
    }

    fn call(&self, task: &str, arguments: &[&str]) -> Result<Value> {
        let registry = Registry::discover().unwrap();
        registry.context(self.root()).call(task, arguments)
    }

    fn releases(&self) -> String {
        fs::read_to_string(self.root().join("releases.md")).unwrap()
    }

    /// Prepare the release as the release task does: generate, then commit
    /// with the `Release:` trailer.
    fn release(&self, version: &str) -> String {
        self.call("releases:update", &[version]).unwrap();
        self.commit(&[], &format!("Bump minor version.\n\nRelease: {version}"))
    }
}

fn expected_section(fixture: &Fixture, kept: &str) -> String {
    let typo = fixture.sha("Fix a typo.");
    format!(
        "**Breaking changes**\n\n- `parse` returns a `Result`. ([#4]({REPOSITORY}/pull/4))\n\n**Changes**\n\n{kept}- Share the task package. ([#6]({REPOSITORY}/pull/6))\n- Fix a typo. ([{}]({REPOSITORY}/commit/{typo}))\n- Change the parser's result. ([#4]({REPOSITORY}/pull/4))\n- Add the parser. ([#2]({REPOSITORY}/pull/2))\n",
        &typo[..7]
    )
}

#[test]
fn generates_the_section_newest_first_from_commits_in_scope() {
    let fixture = Fixture::new(RELEASES);
    let preview = fixture.call("releases:notes", &["Unreleased"]).unwrap();
    assert_eq!(preview, expected_section(&fixture, ""));

    fixture.call("releases:update", &["v0.2.0"]).unwrap();
    assert_eq!(
        fixture.releases(),
        format!(
            "# Releases\n\n## v0.2.0\n\n{}\n## v0.1.0\n\n- First.\n",
            expected_section(&fixture, "")
        )
    );
    assert_eq!(
        fixture.call("releases:notes", &["v0.2.0"]).unwrap(),
        format!("\n{}\n", expected_section(&fixture, ""))
    );
}

#[test]
fn keeps_an_existing_unreleased_section_at_switch_over() {
    let releases = "# Releases\n\n## Unreleased\n\n- Hand-written.\n\n## v0.1.0\n\n- First.\n";
    let fixture = Fixture::new(releases);
    let kept = "- Hand-written.\n";
    assert_eq!(
        fixture.call("releases:notes", &["Unreleased"]).unwrap(),
        expected_section(&fixture, kept)
    );

    fixture.release("v0.2.0");
    assert_eq!(
        fixture.releases(),
        format!(
            "# Releases\n\n## v0.2.0\n\n{}\n## v0.1.0\n\n- First.\n",
            expected_section(&fixture, kept)
        )
    );
    assert_eq!(
        fixture.call("releases:verify", &["v0.2.0"]).unwrap()["commits"],
        4
    );
}

#[test]
fn the_same_history_always_gives_byte_identical_notes() {
    let first = Fixture::new(RELEASES);
    let second = Fixture::new(RELEASES);
    assert_eq!(
        first.git(&["rev-parse", "HEAD"]),
        second.git(&["rev-parse", "HEAD"])
    );
    let preview = first.call("releases:notes", &["Unreleased"]).unwrap();
    assert_eq!(
        first.call("releases:notes", &["Unreleased"]).unwrap(),
        preview
    );
    first.call("releases:update", &["v0.2.0"]).unwrap();
    second.call("releases:update", &["v0.2.0"]).unwrap();
    assert_eq!(
        fs::read(first.root().join("releases.md")).unwrap(),
        fs::read(second.root().join("releases.md")).unwrap()
    );
}

#[test]
fn lists_pending_commits_until_the_release_is_tagged() {
    let fixture = Fixture::new(RELEASES);
    let pending = fixture.call("releases:pending", &[]).unwrap();
    let subjects: Vec<_> = pending
        .as_array()
        .unwrap()
        .iter()
        .map(|commit| commit["subject"].as_str().unwrap())
        .collect();
    assert_eq!(
        subjects,
        [
            "Share the task package. (#6)",
            "Fix a typo.",
            "Change the parser's result. (#4)",
            "Add the parser. (#2)",
        ]
    );
    assert_eq!(pending[0]["sha"], fixture.sha("Share the task package."));

    fixture.release("v0.2.0");
    fixture.git(&["tag", "v0.2.0"]);
    assert_eq!(
        fixture.call("releases:pending", &[]).unwrap(),
        Value::Array(Vec::new())
    );
    fixture.commit(
        &[(".github/dependabot.yml", "version: 2\n")],
        "Configure Dependabot. (#7)",
    );
    assert_eq!(
        fixture.call("releases:pending", &[]).unwrap(),
        Value::Array(Vec::new())
    );
    let error = fixture.call("releases:update", &["v0.3.0"]).unwrap_err();
    assert_eq!(
        error.to_string(),
        "releases:update: no commits in the release scope since v0.2.0; there is nothing to release"
    );
}

#[test]
fn options_override_the_range_scope_and_links() {
    let fixture = Fixture::new(RELEASES);
    let parser = fixture.sha("Add the parser.");
    let notes = fixture
        .call(
            "releases:notes",
            &[
                "Unreleased",
                "--since",
                &parser,
                "--scope",
                "src",
                "--exclude",
                "src/lib.rs",
                "--repository",
                "https://example.com/fork/",
            ],
        )
        .unwrap();
    assert_eq!(
        notes,
        "**Breaking changes**\n\n- `parse` returns a `Result`. ([#4](https://example.com/fork/pull/4))\n\n**Changes**\n\n- Change the parser's result. ([#4](https://example.com/fork/pull/4))\n"
    );
    let pending = fixture
        .call(
            "releases:pending",
            &["--scope", ":(top)benchmarks", "--exclude", ":(exclude)*.md"],
        )
        .unwrap();
    assert_eq!(pending.as_array().unwrap().len(), 0);
}

#[test]
fn chooses_the_newest_earlier_tag_with_the_configured_prefix() {
    let fixture = Fixture::empty();
    let manifest = "[workspace.metadata.bake.release]\ntag-prefix = \"fixture-v\"\n";
    fixture.commit(
        &[("Cargo.toml", manifest), ("releases.md", RELEASES)],
        "Start. (#1)",
    );
    fixture.git(&["tag", "fixture-v0.1.0"]);
    fixture.commit(&[("a.rs", "1\n")], "Candidate. (#2)");
    fixture.git(&["tag", "fixture-v0.2.0-rc.1"]);
    fixture.git(&["tag", "v9.0.0"]);
    fixture.git(&["tag", "fixture-vNext"]);
    fixture.commit(&[("b.rs", "1\n")], "Future. (#3)");
    fixture.git(&["tag", "fixture-v9.0.0"]);
    fixture.git(&["switch", "--quiet", "--create", "side"]);
    fixture.commit(&[("c.rs", "1\n")], "Side. (#4)");
    fixture.git(&["tag", "fixture-v0.3.0"]);
    fixture.git(&["switch", "--quiet", "main"]);
    fixture.commit(&[("d.rs", "1\n")], "Final. (#5)");

    let subjects = |arguments: &[&str]| -> Vec<String> {
        let pending = fixture.call("releases:pending", arguments).unwrap();
        pending
            .as_array()
            .unwrap()
            .iter()
            .map(|commit| commit["subject"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(subjects(&[]), ["Final. (#5)"]);
    fixture
        .call("releases:update", &["fixture-v0.2.0"])
        .unwrap();
    assert!(
        fixture
            .releases()
            .starts_with("# Releases\n\n## fixture-v0.2.0\n\n- Final. (#5)\n- Future. (#3)\n\n")
    );
}

#[test]
fn uses_the_whole_history_without_a_previous_tag() {
    let fixture = Fixture::empty();
    fixture.commit(&[("releases.md", "# Releases\n")], "Start.");
    fixture.call("releases:update", &["v1"]).unwrap();
    assert_eq!(fixture.releases(), "# Releases\n\n## v1\n\n- Start.\n");
}

#[test]
fn verifies_the_release_against_the_history_of_its_first_parent() {
    let fixture = Fixture::new(RELEASES);
    fixture.git(&["switch", "--quiet", "--create", "release-v0.2.0"]);
    let release = fixture.release("v0.2.0");
    assert_eq!(
        fixture
            .call("releases:verify", &["v0.2.0", "--sha", &release])
            .unwrap(),
        serde_json::json!({"version": "v0.2.0", "commits": 4})
    );

    // A feature merged while the release was open makes the merge candidate stale.
    fixture.git(&["switch", "--quiet", "main"]);
    fixture.commit(&[("src/late.rs", "// Late.\n")], "Add a late feature. (#8)");
    fixture.git(&["merge", "--quiet", "--no-ff", "--no-edit", "release-v0.2.0"]);
    let error = fixture.call("releases:verify", &["v0.2.0"]).unwrap_err();
    assert!(
        error.to_string().contains(&format!(
            "line 7 should be \"- Add a late feature. ([#8]({REPOSITORY}/pull/8))\" but is \"- Share the task package."
        )),
        "{error}"
    );

    // A hand edit to the generated section is reported too.
    fixture.git(&["switch", "--quiet", "release-v0.2.0"]);
    let edited = fixture
        .releases()
        .replace("- Fix a typo.", "- Fix a typo, by hand.");
    fixture.write("releases.md", &edited);
    fixture.git(&["commit", "--quiet", "--amend", "--no-edit", "--all"]);
    let error = fixture
        .call(
            "releases:verify",
            &["v0.2.0", "--sha", "HEAD", "--since", "v0.1.0"],
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("line 8 should be \"- Fix a typo."),
        "{error}"
    );
}

#[test]
fn verification_reports_missing_inputs() {
    let fixture = Fixture::new(RELEASES);
    let root = fixture.git(&["rev-list", "--max-parents=0", "HEAD"]);
    for (arguments, message) in [
        (vec!["v0.2.0", "--sha", "missing"], "missing^{commit}"),
        (vec!["v0.2.0", "--sha", root.as_str()], "^1^{commit}"),
        (vec!["v0.2.0", "--since", "missing"], "missing^{commit}"),
        (vec!["v0.2.0", "--since", "HEAD^"], "nothing to release"),
        (vec!["v0.1.0"], "\"v0.1.0\" already exists"),
        (vec!["v0.2.0"], "release heading \"v0.2.0\" not found"),
    ] {
        let error = fixture.call("releases:verify", &arguments).unwrap_err();
        assert!(
            error.to_string().contains(message),
            "{arguments:?}: {error}"
        );
    }

    // The document is missing from the candidate, then from its parent.
    fs::remove_file(fixture.root().join("releases.md")).unwrap();
    fixture.commit(&[], "Remove the notes.");
    let error = fixture.call("releases:verify", &["v0.2.0"]).unwrap_err();
    assert!(error.to_string().contains(":./releases.md"), "{error}");
    fixture.commit(&[("releases.md", RELEASES)], "Restore the notes.");
    let error = fixture.call("releases:verify", &["v0.2.0"]).unwrap_err();
    assert!(error.to_string().contains(":./releases.md"), "{error}");
}

#[test]
fn checks_that_only_release_commits_edit_the_document() {
    let fixture = Fixture::new(RELEASES);
    let base = fixture.git(&["rev-parse", "HEAD"]);
    for base in ["", "0000000000000000000000000000000000000000"] {
        assert_eq!(
            fixture.call("releases:check", &[base]).unwrap(),
            serde_json::json!({"changed": false, "release": false})
        );
    }
    fixture.commit(&[("src/new.rs", "// New.\n")], "Add a feature. (#9)");
    assert_eq!(
        fixture.call("releases:check", &[&base]).unwrap(),
        serde_json::json!({"changed": false, "release": false})
    );
    let edited = format!("{RELEASES}- By hand.\n");
    fixture.commit(&[("releases.md", &edited)], "Edit the notes by hand. (#10)");
    let error = fixture.call("releases:check", &[&base]).unwrap_err();
    assert!(
        error.to_string().contains(": releases.md is generated"),
        "{error}"
    );
    let release = fixture.release("v0.2.0");
    assert_eq!(
        fixture
            .call("releases:check", &[&base, "--head", &release])
            .unwrap(),
        serde_json::json!({"changed": true, "release": true})
    );
    assert!(
        fixture
            .call("releases:check", &[&base, "--head", "missing"])
            .is_err()
    );
    assert!(fixture.call("releases:check", &["missing"]).is_err());

    // A base with no common history cannot be compared.
    fixture.git(&["switch", "--quiet", "--orphan", "unrelated"]);
    let unrelated = fixture.commit(&[("other.md", "Other.\n")], "Unrelated.");
    fixture.git(&["switch", "--quiet", "main"]);
    let error = fixture.call("releases:check", &[&unrelated]).unwrap_err();
    assert!(error.to_string().contains(": git diff"), "{error}");
}

#[test]
fn generation_reports_git_and_manifest_errors() {
    let empty = Fixture::empty();
    empty.write("releases.md", RELEASES);
    for arguments in [vec!["Unreleased"], vec!["Unreleased", "--since", "HEAD"]] {
        assert!(
            empty.call("releases:notes", &arguments).is_err(),
            "{arguments:?}"
        );
    }

    let fixture = Fixture::new(RELEASES);
    for (task, arguments) in [
        ("releases:notes", vec!["Unreleased", "--since", "missing"]),
        ("releases:update", vec!["v0.2.0", "--since", "missing"]),
        ("releases:update", vec!["v0.1.0"]),
        ("releases:pending", vec!["--since", "missing"]),
        ("releases:pending", vec!["--scope", ":(unknown-magic)src"]),
    ] {
        assert!(
            fixture.call(task, &arguments).is_err(),
            "{task} {arguments:?}"
        );
    }
    assert_eq!(fixture.releases(), RELEASES);

    fixture.write("Cargo.toml", "[package\n");
    assert!(fixture.call("releases:pending", &[]).is_err());
}

# Releases

## Unreleased

- Generate each version's section of `releases.md` from Git history, so pull requests no longer edit it. `releases:update` lists the subject of each commit since the previous release tag, newest first and linked to its pull request, under any **Breaking changes** taken from `Breaking:` paragraphs in commit bodies, and keeps the body of an existing `## Unreleased` section. `releases:notes Unreleased` previews the section, `releases:pending` lists its commits, `releases:verify` checks a release candidate's section against its history, and `releases:check` rejects hand edits in pull requests. `insert_notes` writes generated `Notes` into a document, and `update_document` is deprecated.

## v0.3.9

- Require `bake` 0.20 or later, instead of 0.18 or later, so that Cargo resolves this crate to the same `bake` as a task package that requires `bake` 0.20.2. In a workspace whose `rust-version` is below the 1.88 that `bake` 0.20.2 declares, Cargo resolved the old requirement to `bake` 0.19.2, which declares none, and the task package failed to compile against two `bake` versions.

## v0.3.8

- Declare Rust 1.88 as the minimum supported version in `rust-version`, as required by the let-chains in `bake` 0.20.

## v0.3.7

- Depend on `base-markdown` under the `base-rs` organization, replacing `core-markdown`.

## v0.3.6

- Migrate to the source-backed `core-markdown` document AST, preserving supported Markdown behavior and source bytes.

## v0.3.5

- Use `core-markdown` in place of `socketry-markdown`.

## v0.3.4

- Clarify root task exports, installation, and the shared release process.

## v0.3.3

- Resolve development task dependencies to the current checkout.

- Adopt `socketry-project` 0.3.7 for shared project tasks and Markdown normalization.

- Require the aggregate test and coverage result for pull request merges.

## v0.3.2

- Require Bake 0.18 or newer so task libraries share their project's active task registry.
- Document shared agent context setup and contribution guidance.

## v0.3.1

- Require Bake 0.18.0 for the shared task registry.

## v0.3.0

- Remove the redundant `bake_releases::releases` module; task adapters are now available at the crate root.

## v0.2.4

- Declare compatibility with the Bake 0.x API so task libraries can share one task registry when upgrading to crate-derived task namespaces.

## v0.2.3

- Extract and update release sections using parsed Markdown headings.

## v0.2.2

- Create or update GitHub Releases after successful crates.io publication.
- Resolve the local task crate during version updates.

## v0.2.1

- Switch the runtime dependency from `socketry-bake` to `bake` 0.17.0.

- Add Bake Agent Context tasks to the project's development executable.

- Link the shared Rust context guidance from the README.

## v0.2.0

- Keep this crate focused on release-document extraction and editing; GitHub Release creation now lives in `bake-cargo`.

## v0.1.0

- Move the reusable releases.md task library into its own repository.
- Add an explicit task for creating GitHub Releases from release notes.

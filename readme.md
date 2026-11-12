# `bake-releases`

Reusable releases.md tasks for Bake, which generate each version's release notes from Git history, inspired by Samuel Williams's [Ruby bake-releases](https://github.com/ioquatix/bake-releases) (MIT).

## Usage

The task functions are exported from the crate root and register beneath `releases`. Add the dependency to the private task binary and regenerate its links:

```sh
cargo bake --regenerate
cargo add --manifest-path bake/Cargo.toml bake-releases
cargo bake --regenerate
```

No per-task registration calls are needed. `#[bake::task]` functions in the library are collected by `Registry::discover()`.

```sh
cargo bake releases:notes Unreleased
cargo bake releases:update v0.1.0
cargo bake releases:notes v0.1.0 --path releases.md
```

This crate only manages the Markdown release document. Cargo publication and GitHub release creation are provided by [Bake Cargo](https://github.com/base-rs/bake-cargo).

## Generated release notes

Nobody edits `releases.md` by hand. When a release is prepared, `releases:update vX.Y.Z` writes the version's section from Git history, so pull requests never conflict on the file:

- One line for each commit on the first-parent history since the previous release tag, newest first, holding its subject verbatim. With squash merges the subject is the pull request title, so write titles for users. A trailing `(#N)` becomes a link to the pull request, and a commit without one links to itself. Links are built from `--repository URL`, or else the manifest's `repository`; without either, subjects are kept as they are.
- A **Breaking changes** list at the top, from every paragraph of a commit body (the pull request description) that begins `Breaking:`, followed by a **Changes** lead-in. Say what breaks and how to migrate.
- Commits with a `Release:` trailer are skipped, and so are commits that touch only `.github/`, `.agents/` or the private `bake/` task package. List more paths, relative to the repository root, in `[workspace.metadata.bake.release] notes-exclude`, or pass `--exclude PATH` or `--scope PATHSPEC`.
- The previous release is the highest tag reachable from `HEAD` that is the manifest's `tag-prefix` (`v` by default) followed by a semantic version below the new one. Override it with `--since REV`; without one, the whole history counts.

The same history always gives byte-identical notes, already in the form `markdown:normalize` writes. An existing `## Unreleased` section is replaced, and its body is kept between the breaking changes and the generated lines, so notes written before the switch are not lost. `update` fails when there are no commits to describe.

```sh
cargo bake releases:notes Unreleased     # Preview the next section without writing it.
cargo bake releases:pending              # List the commits it describes; empty means nothing to release.
cargo bake releases:verify v0.2.0 --sha "$CANDIDATE"
cargo bake releases:check "$BASE"
```

`releases:verify` regenerates the section from the history of the candidate's first parent, which for a squash or merge candidate is the target branch, and fails unless it matches the committed `## vX.Y.Z` section. This catches hand edits and features merged after the release was prepared. `releases:check BASE` fails when `HEAD` (or `--head REV`) changes `releases.md` since its merge base with `BASE` without a `Release:` trailer; run it in pull request CI.

## Document format

Use unindented ATX headings such as `## v0.1.0`. The version argument matches the complete heading title exactly (including any v prefix). Optional closing heading markers are supported. Fenced code blocks can contain example headings. Setext headings, HTML blocks, and headings inside lists or block quotes are outside this release-document format.

notes returns the body beneath the selected heading until the next heading of the same or a higher level. Nested sections, whitespace, line endings, and other Markdown bytes are preserved. Missing and duplicate headings are errors.

update inserts the new section before the first `##` heading, or at the end. It rejects an existing release heading, duplicate Unreleased sections, and invalid multiline titles. The rest of the file is preserved, and the new section uses the file's line endings. A temporary file in the same directory is written and synchronized before replacing the original, preserving file permissions. Existing symlinks are followed. As with other file editors, concurrent external edits are not merged and replacing a file changes its identity for hard links.

These tasks do not modify Cargo versions, commit, tag, or publish releases. The pure `extract_notes`, `render_section` and `update_document` functions, with `Notes` and `Commit`, are also available for direct library use.

## Releasing

Prepare a release with `cargo bake cargo:release:patch` (or `minor`, `major`, or `bump --version X.Y.Z`) from a clean checkout of `main`; it bumps the version, commits with the `Release: vVERSION` trailer, validates the candidate, and opens the pull request. After review and merge, GitHub Actions publishes the release when the configured `crates-io` environment approves it. Follow the shared [Releasing skill](https://github.com/base-rs/base-project/blob/main/context/releasing.md) for the standard process.

## Releases

<!-- bake-readme:releases:start -->

See [releases.md](releases.md) for the full release history.

### v0.3.9

- Require `bake` 0.20 or later, instead of 0.18 or later, so that Cargo resolves this crate to the same `bake` as a task package that requires `bake` 0.20.2. In a workspace whose `rust-version` is below the 1.88 that `bake` 0.20.2 declares, Cargo resolved the old requirement to `bake` 0.19.2, which declares none, and the task package failed to compile against two `bake` versions.

### v0.3.8

- Declare Rust 1.88 as the minimum supported version in `rust-version`, as required by the let-chains in `bake` 0.20.

### v0.3.7

- Depend on `base-markdown` under the `base-rs` organization, replacing `core-markdown`.

<!-- bake-readme:releases:end -->

## See Also

- [`bake`](https://github.com/base-rs/bake).

- [`bake-cargo`](https://github.com/base-rs/bake-cargo).

## Contributing

Please open an issue or pull request on [GitHub](https://github.com/base-rs/bake-releases).

### Agent Context

Run `cargo bake agent:context:install` to install shared context and skills. Read `.agents/context/index.md` to find relevant guides, follow `agents.md` if present, and apply skills under `.agents/skills/`. The installer preserves repository-owned `agents.md`; it does not create or regenerate that file.

[Agent Context guide]: https://github.com/base-rs/bake-agent-context/blob/main/context/agent-context.md

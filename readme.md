# `bake-releases`

Reusable releases.md tasks for Bake, inspired by Samuel Williams's [Ruby bake-releases](https://github.com/ioquatix/bake-releases) (MIT).

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

This crate only manages the Markdown release document. Cargo publication and GitHub release creation are provided by [Bake Cargo](https://github.com/core-rs/bake-cargo).

## Document format

Use unindented ATX headings such as `## Unreleased` and `## v0.1.0`. The version argument matches the complete heading title exactly (including any v prefix). Optional closing heading markers are supported. Fenced code blocks can contain example headings. Setext headings, HTML blocks, and headings inside lists or block quotes are outside this release-document format.

notes returns the body beneath the selected heading until the next heading of the same or a higher level. Nested sections, whitespace, line endings, and other Markdown bytes are preserved. Missing and duplicate headings are errors.

update renames exactly one Unreleased heading. It rejects an existing release heading, missing/duplicate Unreleased sections, and invalid multiline titles. The rest of the file is preserved. A temporary file in the same directory is written and synchronized before replacing the original, preserving file permissions. Existing symlinks are followed. As with other file editors, concurrent external edits are not merged and replacing a file changes its identity for hard links.

These tasks do not modify Cargo versions, create a new Unreleased section, commit, tag, or publish releases. The pure `extract_notes` and `update_document` functions are also available for direct library use.

## Releasing

Prepare a release with `cargo bake cargo:version:patch` (or `minor`, `major`, or `bump --version X.Y.Z`), commit with a `Release: vVERSION` trailer, then run `cargo bake cargo:release` and open a pull request. After review and merge, GitHub Actions publishes the release when the configured `crates-io` environment approves it. Follow the shared [Releasing skill](https://github.com/core-rs/core-project/blob/main/context/releasing.md) for the standard process.

## Releases

<!-- bake-readme:releases:start -->

See [releases.md](releases.md) for the full release history.

### v0.3.5

- Use `core-markdown` in place of `socketry-markdown`.

### v0.3.4

- Clarify root task exports, installation, and the shared release process.

### v0.3.3

- Resolve development task dependencies to the current checkout.

- Adopt `socketry-project` 0.3.7 for shared project tasks and Markdown normalization.

- Require the aggregate test and coverage result for pull request merges.

<!-- bake-readme:releases:end -->

## See Also

- [`bake`](https://github.com/core-rs/bake).

- [`bake-cargo`](https://github.com/core-rs/bake-cargo).

## Contributing

Please open an issue or pull request on [GitHub](https://github.com/core-rs/bake-releases).

### Agent Context

Run `cargo bake agent:context:install` to install shared context and skills. Read `.agents/context/index.md` to find relevant guides, follow `agents.md` if present, and apply skills under `.agents/skills/`. The installer preserves repository-owned `agents.md`; it does not create or regenerate that file.

[Agent Context guide]: https://github.com/core-rs/bake-agent-context/blob/main/context/agent-context.md

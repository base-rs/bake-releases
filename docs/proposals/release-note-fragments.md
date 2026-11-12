# Proposal: Release-Note Fragments

Status: **proposal for discussion**, November 2026. Nothing here is implemented. If it is agreed, the requirements move into this crate's readme and the Base skills, which then take precedence over this document.

## The problem

Base projects keep their release notes in `releases.md`. Each pull request with a user-visible change adds its entry at the top of the same `## Unreleased` section, so any two pull requests in flight conflict. A merge queue then marks the second one unmergeable until someone rebases it and resolves the conflict by hand. This happened to base-rs/tomori#398.

A `.gitattributes` `merge=union` driver does not fix it. GitHub ignores repository merge drivers on the server side: in pull request conflict checks, the web editor, merges and the merge queue ([community discussion 9288](https://github.com/orgs/community/discussions/9288), unanswered since 2021; also confirmed in a scratch repository). It would also misorder entries and could join sections even where it did run.

## How `releases.md` is used today

- **Format.** `# Releases`, then `## Unreleased`, then `## vX.Y.Z` sections, newest first. Each entry is a Markdown list item. Some projects use tight lists (`base-project`, this crate). Tomori uses loose lists of long paragraphs, sometimes with nested lists, and marks incompatible changes inline (`**Incompatible changes to the public API.**`). There are no per-crate or per-kind sections: a workspace shares one version, one `releases.md` and one tag.
- **`cargo:release:patch|minor|major|bump`** (`bake-cargo`) bumps the version, then calls the `cargo:after_version_bump` hook (`base-project`). The hook runs `releases:update TAG` (this crate), which renames the one `## Unreleased` heading to `## TAG`, then `readme:update` and `markdown:normalize`. The task then runs `git commit --all` with the `Release: TAG` trailer. `cargo:release` checks that a `## TAG` heading exists and extracts its body with `extract_notes`, and the release pull request's body is that text followed by the trailer.
- **`cargo:release:detect`** reads the trailer and requires a `## TAG` heading in `releases.md`.
- **`cargo:publish:pending`** only checks crates.io and never reads the notes.
- **`cargo:release:publish`** publishes, tags, and then creates or updates the GitHub Release from the `## TAG` section through `cargo:releases:github:release`.
- **Guidance.** The `base-project-pull-requests` skill says to add a brief entry to `releases.md` for user-visible changes. The `base-project-releasing` skill says to check that `## Unreleased` describes the release before starting one.

Every consumer after the hook reads only the `## TAG` section. If the fragments are gathered into that section during the hook, nothing downstream has to change.

## Proposal

### Directory and file names

Unreleased entries are files in `releases/unreleased/`, one per pull request, named after the branch: `releases/unreleased/<branch>.md`, with `/` in the branch name replaced by `-`. Two open pull requests never touch the same path, so they cannot conflict. Any other unique name also works, because the name is never used for ordering or shown to readers. Only `*.md` files directly inside the directory count, and dotfiles are ignored. Git does not track empty directories, so the directory disappears after each release and the next contributor creates it again.

`base-project` already keeps the history of a predecessor project in `releases/socketry-project.md`. Only `releases/unreleased/` is read, so the two can coexist.

### File format

A fragment is the Markdown you would otherwise have added under `## Unreleased`: one or more top-level list items, which may contain paragraphs and nested lists.

```markdown
- The swarm now speaks for as long as you hear the agent. See `design/speech.md`.
```

- No front matter, kinds or crate scoping. Base notes are not grouped that way, and a breaking change stays an inline bold lead-in, as Tomori writes it today. A fragment that begins with `---` is rejected, so front matter can be given a meaning later without ambiguity.
- Headings are rejected, since they would split the release section. A fragment that is not a list is also rejected, and so is an empty one.

### Ordering

Gathered entries go newest first, which matches the existing convention. A fragment's time is the committer time of the first commit on the first-parent history of `HEAD` that added its path: `git log --first-parent --diff-filter=A --format=%ct -- PATH`, taking the oldest result. With squash merges, which Base repositories use, that commit is the merge into `main`, so the order is the order of merging. It is deterministic for a given history and does not change when a fragment is edited later. Ties are broken by path.

A fragment that Git does not track is an error. The release task's clean-tree check ignores untracked files, and `git commit --all` would not commit such a file's deletion.

### What the release task does

`releases:update TAG` gains the gathering step, so `cargo:release:*` and the hook keep their current interface:

1. Read the tracked fragments, validate them all, and order them.
2. Build the new section: the fragments, then any entries still under `## Unreleased`. Separate the entries the way the document's latest list does: with a blank line if it is loose, and with a plain line break otherwise.
3. Rename `## Unreleased` to `## TAG` with that body. If there is no `## Unreleased`, insert `## TAG` before the first `##` heading. If there are no fragments and no `## Unreleased`, fail as today.
4. Write `releases.md` atomically as today, then delete the fragments. `git commit --all` stages the deletions, so the gathering and the removal land in the same release commit.

The pull request body, `cargo:release:detect`'s heading check, `readme:update` and the GitHub Release all keep reading `## TAG`, unchanged.

`releases:notes Unreleased` returns the same gathered body without writing anything. Maintainers can use it to preview the next release, and the `base-project-releasing` skill's "check `## Unreleased` before releasing" becomes "check `cargo bake releases:notes Unreleased`".

### A fragment merged while the release is open

Today, a feature pull request that adds to `## Unreleased` while a release pull request renames it either conflicts or leaves its entry above the new version. With fragments it merges cleanly, but its note would then wait for the next release even though its code ships in this one.

To prevent that, `cargo:release:detect` additionally requires `releases/unreleased/` to hold no fragments in a release candidate. The publish workflow tests the merge candidate, so the release pull request fails its `check` in the queue. The fix is to rebase the release branch and run `releases:update TAG` again. To make that possible, `releases:update` also accepts a document whose first `##` heading is already `## TAG`, and adds the new fragments to the top of that section.

### Validation in pull requests

A new task, `releases:check`, validates the fragments (format, tracked, no headings or front matter) and the release document's headings. The canonical `test.yml` (from the `bake-test` context) runs it beside `license:check`, so a malformed fragment fails its own pull request rather than the release.

Requiring a fragment in every pull request is not proposed. Whether a change is user-visible is a judgement for review, and the skills already ask for it. An opt-in `releases:check --base REV --require` could come later if projects want it.

### Editing a fragment later

Before a release, edit the fragment in a follow-up pull request. Only pull requests that edit the same entry can conflict, which is the right granularity. After a release, edit the published section of `releases.md` as today. `cargo:releases:github:release` already updates a GitHub Release whose notes have changed.

### Migration

- A project without `releases/unreleased/` behaves exactly as today.
- During the transition, both sources are valid. Entries under `## Unreleased` are kept after the fragments, so pull requests already open still release correctly.
- Once a project has adopted fragments, it removes its empty `## Unreleased` heading. `releases:check` could then warn when one reappears, but that is optional.
- The changes are in `bake-releases` (gathering in `update` and `notes`, and `releases:check`), `bake-cargo` (the detect check), `base-project` (raising its `bake-releases` requirement), and the canonical `test.yml` in the `bake-test` context, which gains a `releases:check` step beside its `license` job.

### Instructions for contributors and agents

- **`base-project-pull-requests`, Release notes:** "For user-visible changes, add `releases/unreleased/<branch>.md` with your entry as Markdown list items; do not edit `releases.md`."
- **`base-project-releasing`:** preview with `releases:notes Unreleased`. The release commit gathers and deletes the fragments, and if detect reports leftover fragments, you rebase and run `releases:update` again.
- **`base-project` `conventions.md` and `setup.md`:** they mention "update `releases.md`" in passing, and those lines change to name the fragment directory.
- **This crate's readme:** document the fragment format, the ordering and `releases:check`.
- Projects' own `agents.md` files only need changes where they repeat the old instruction. Tomori's does not.

## Alternatives considered

- **A `merge=union` driver:** GitHub ignores it on the server side, and it misorders entries.
- **Generating notes from pull request descriptions or commit messages:** a Base description is a one-paragraph commit message written for reviewers, while notes are written for users and are often longer, as Tomori's are. Not every pull request is user-visible. Generation would also need the GitHub API at release time, and the text would not be reviewed in the diff.
- **A bot that rebases conflicting pull requests:** it still serializes merges, churns CI and the merge queue, and resolves prose conflicts mechanically. It also needs a token with push rights.
- **Inserting entries at random places or at the bottom of `## Unreleased`:** this only lowers the conflict rate, and it loses ordering.
- **Adopting towncrier or changesets directly:** both are the precedent for this design (as are GitLab's changelog entries and ClickHouse's `AGENTS.md`), but they bring Python or Node and their own templates into a Cargo-only toolchain. The part Base needs is about a hundred lines in `releases:update`.

Fragments win because they remove the shared edit point entirely, keep notes reviewed in the diff of the pull request they describe, and change nothing after the release commit.

## Open questions

- Is `releases/unreleased/` the right path, given `base-project`'s existing `releases/` history file? `releases.d/` is an alternative.
- Should a fragment be allowed to state its own position, for example to put a breaking change first? This proposal leaves it to merge order and to editing at release time.

# Proposal: Release Notes Generated from Git History

Status: **accepted**, November 2026, and implemented in this crate; the readme and `context/release-notes.md` describe the behavior and take precedence over this document. The bake-cargo, base-project and skill changes below are follow-ups in those repositories.

## Implementation notes

These details differ from, or settle, the proposal below:

- `releases:update VERSION` needs no new options: the previous tag, scope and repository URL default to what the hook would pass, from the manifest's `tag-prefix`, `notes-exclude` and `repository`, so `after_version_bump` works unchanged. `--since`, `--scope`, `--exclude` and `--repository` override them.
- Detection is `releases:verify VERSION --sha CANDIDATE`, which bake-cargo's `cargo:release:detect` calls. It regenerates from the candidate's first parent and compares normalized Markdown, since the hook runs `markdown:normalize` after writing.
- `releases:check BASE` judges the non-merge commits in `BASE..HEAD` that change `releases.md` rather than `HEAD`'s own trailer, so GitHub's pull request merge commit and a merge queue's commit, which carry none, pass for a release. The `Breaking:` marker matches in any case.
- Generated sections are already normalized, so the hook leaves them unchanged. Subjects stay verbatim apart from the escapes the normalizer adds, such as `\*`.
- A **Changes** lead-in follows the breaking changes, because two lists separated only by a blank line are one list in Markdown. Kept `## Unreleased` entries continue the generated list. A kept body with its own **Breaking changes** lead-in, as Tomori's has, shares one breaking list with the generated ones, kept entries first, so the section never has two. Duplicates are not removed: a pull request number is no proof that two entries describe the same change.
- The new section is written by `insert_notes(document, version, notes)`, with `Notes` built by `Notes::from_commits(commits, repository)`; `render_section(commits, repository)` is the pure renderer. `update_document(document, version)` keeps its signature and is deprecated, so this ships as a compatible 0.3 release: bake-cargo requires `bake-releases` 0.3, and a 0.4 beside base-project's open-ended requirement would link two copies whose tasks collide.

## The problem

Each pull request with a user-visible change adds a line to the same `## Unreleased` section of `releases.md`, so any two pull requests in flight conflict, and a merge queue marks the second one unmergeable until someone rebases it by hand (base-rs/tomori#398). A `.gitattributes` `merge=union` driver does not help: GitHub ignores merge drivers on the server side ([community discussion 9288](https://github.com/orgs/community/discussions/9288)).

## Decision

Nobody edits `releases.md` by hand any more. At release time, bake writes the new version's section from Git history: one line for each commit since the previous release tag, which is the squash-merged title of each pull request. Only release commits touch `releases.md`, so feature pull requests never conflict on it. No agent or language model rewrites the text, so each line maps 1:1 to a commit and the same history always gives the same notes.

## The generated section

**Range.** The commits are `git log --first-parent PREVIOUS..HEAD -- SCOPE`, run by the release hook before the release commit is made. `PREVIOUS` is the newest tag with the project's tag prefix whose version parses and which is an ancestor of `HEAD`. Commits with a `Release:` trailer are skipped. A project with no previous tag uses its whole history. `SCOPE` is described under [Scope](#scope).

**Ordering: newest first.** Versions in `releases.md` are already newest first, and so are the entries within each section today. It is also `git log`'s own order, so the section and `git log` can be compared line by line. A follow-up that fixes a feature in the same release comes just above the feature.

**Line format.** Each line is a list item holding the commit subject verbatim. A trailing `(#N)`, which GitHub adds to squash subjects, becomes a link to the pull request. A commit without one, such as a direct push, gets a link to the commit. Links are built from the manifest's `repository` URL. If the manifest has no `repository`, `(#N)` is left as it is, and GitHub still links it in the GitHub Release.

```markdown
## v0.3.0

**Breaking changes**

- `Server::builder()` registers no environment but the grid; register the ones you offer with `.environment(...)`. ([#197](https://github.com/base-rs/tomori/pull/197))

**Changes**

- Make the campsite's marshmallows twice as big. ([#398](https://github.com/base-rs/tomori/pull/398))
- Add the farmyard environment, an evening yard with a chicken coop, in a common and a client crate that Tomori's own programs register. ([#197](https://github.com/base-rs/tomori/pull/197))
```

The breaking-change block is a bold lead-in paragraph rather than a `###` heading. `readme:update` copies a section's body under its own `### vX.Y.Z` heading, so a nested `###` would come out as a sibling of the version heading there.

## Breaking changes

A pull request that breaks compatibility adds a paragraph that begins `Breaking:` to its description. The paragraph says what breaks and how to migrate, and a description may contain several. With the repository setting below, the description becomes the squash commit's body. The generator takes every body paragraph that begins with `Breaking:`, joins its lines with spaces, strips the marker, and lists the result under **Breaking changes** at the top of the section, linked like the commit's own line. The commit still has its line in the main list.

**A paragraph marker, not a Git trailer.** A trailer (`Breaking-Change:`) would match `Release:`, but Git only reads trailers from the last paragraph of a message. GitHub can append a co-author paragraph (`Co-authored-by:`) after the description, which would silently hide a trailer written at the end of the description. A paragraph marker survives that, can sit anywhere in the description, and reads as plain prose to a reviewer. The `Release:` trailer stays as it is, because the release task writes it and release pull requests have a single author.

**Repository setting (recommended, not changed here).** Set the default squash merge message to "Pull request title and description" (`squash_merge_commit_title: PR_TITLE`, `squash_merge_commit_message: PR_BODY`); the merge queue's squash merges use the same message. Also allow only squash merges. A rebase merge would add one line per branch commit, with no `(#N)`. base-rs/tomori already has the title and description setting, and its history shows it. It still allows rebase merges, in the repository settings and in its pull request ruleset (`allowed_merge_methods: squash, rebase`), even though its merge queue squashes. The `base-project-github-repository` skill should require squash only.

## Scope

Base workspaces release together: one version, one root `releases.md` and one tag (`setup.md`, `conventions.md`). Tomori is one: one workspace version (0.2.0), one tag series (`v0.2.0`), and release notes only in the root `releases.md` and readme; its crates' readmes carry no notes. So the release unit is the workspace, and per-crate scope is a path rule that works the same way for a single package.

- **Workspace release (Tomori, and Base today):** a commit counts if it changes any path outside the repository's maintenance paths.
- **Package release (a single-crate repository, or a future workspace member with its own tag prefix and `releases.md`):** a commit counts if it changes the package's directory or a workspace build input.
- **Workspace build inputs** are the root `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` and `.cargo/`. They count for every package, because a dependency or toolchain change changes what every package is built from.
- **Maintenance paths** are `.github/`, `.agents/` and the private `bake/` task package. A commit that touches only these appears in no notes, as CI-only commits such as Tomori's #401 should not. A commit that also touches anything else counts as usual. Projects can extend the list with `[workspace.metadata.bake.release] notes-exclude = [...]`, given as Git pathspecs.

The scope is a Git pathspec, for example `:(top)crates/scene :(top)Cargo.toml :(top)Cargo.lock :(exclude,top).github`. Git's own path filter therefore decides which commits count, and nothing is reimplemented.

## What changes, by function

**bake-releases (this crate).**

- `document::update_document(document, version)` becomes `update_document(document, version, section)`. It inserts `## VERSION` with the generated body before the first `##` heading. If an `## Unreleased` heading exists, the body under it is kept, as described in [Migration](#migration). It still rejects an existing `## VERSION`.
- A new pure function, `document::render_section(commits, repository)`, takes the commits (SHA, subject and body) and returns the Markdown. It is unit-tested without Git.
- A new `history` module runs the `git log` query above and parses `%H%x00%s%x00%b%x1e`.
- The `update` task gains `--since REV`, `--scope PATHSPEC...` and `--repository URL`, then calls `history` and `render_section`. It fails when the range has no commits.
- The `notes` task: `releases:notes Unreleased` (with the same options) previews the next section without writing anything. `extract_notes` is unchanged.
- A new task, `releases:pending --since REV`, lists the commits in scope, so an empty result means there is nothing to release.
- A new task, `releases:check --base REV`, fails when `REV..HEAD` changes `releases.md` and `HEAD` has no `Release:` trailer. The canonical `test.yml` runs it beside `license:check`, with the pull request's or merge group's base SHA, so a hand edit fails its own pull request.

**base-project.** `after_version_bump` calls `releases:update TAG --since PREVIOUS --scope ... --repository URL`. It works these out with a new `bake_cargo::previous_release_tag(context)`, which runs `git tag --merged HEAD --list 'PREFIX*'` and takes the highest version, and with the scope rule and manifest metadata above. The rest of the hook is unchanged.

**bake-cargo.**

- `release::start` calls `releases:pending` before bumping and refuses to start when there is nothing to release, which replaces today's failure on a missing `## Unreleased`.
- `release::package`, `contains_release_heading` and the release pull request body are unchanged: they read `## TAG`, which is now generated.
- `workflow::detect_release` keeps the `Release:` trailer logic and the `## TAG` check. It also regenerates the section for the candidate's first parent and requires it to equal the committed `## TAG` body. This catches a hand edit, and also a feature merged while the release pull request was open, since the merge queue tests the squashed candidate on top of it. The fix is to rebase the release branch and run the hook again. `publish.yml` already checks out with `fetch-depth: 0`, so the tags and history are there.
- `releases::github::release` is unchanged. It already sends the `## TAG` section as `--notes-file` and never uses `--generate-notes`, so the GitHub Release, the release pull request and `releases.md` show the same text.

## Migration

- Existing release sections are history and stay as they are.
- At the first release after the switch, an existing `## Unreleased` body is kept as written. It goes after the **Breaking changes** block and before the generated list, and the `## Unreleased` heading is removed. Nothing is lost, and the hand-written entries overlap the generated lines for those commits for this one release only. Detection reproduces this from the parent's `releases.md`. From the switch-over on, `releases:check` stops new entries from being added by hand.
- Commit subjects become user-facing notes, so pull request titles must stay clear, complete sentences about what changed for users. Tomori's already are: 196 of the 197 commits since `v0.2.0` end in `(#N)`. That release will have about 200 lines.
- Tomori's manifest `repository` is `https://github.com/ioquatix/tomori`, which GitHub redirects. It should name `base-rs/tomori` before the links are generated.
- **Skills and instructions.**
  - In `base-project-pull-requests`, *Release notes* becomes: "Do not edit `releases.md`. The pull request title becomes this change's release note, so write it for users. If the change breaks compatibility, add a paragraph to the description beginning `Breaking:` that says what breaks and how to migrate." The rule against ending descriptions with a `Key: value` line gains that exception.
  - In `base-project-releasing`, "check `## Unreleased`" becomes "preview the notes with `cargo bake releases:notes Unreleased`; there is nothing to edit, because the lines come from history, so a poor line is fixed by writing clearer titles from then on."
  - The passing mentions in `setup.md`, `conventions.md` and `update.md`, bake-cargo's readme and this crate's readme change to match.
  - Tomori's `agents.md` and `CLAUDE.md` do not mention `releases.md`, so they need no changes.

## Alternatives considered

- **Fragments** (`releases/<branch>.md`, flat, or `releases/unreleased/`), the earlier draft of this proposal. They remove the conflicts, but they are still hand-written notes kept apart from the commits they describe. A pull request can forget its fragment or describe something it does not do, ordering needs history queries anyway, and they bring validation, gathering and deletion steps.
- **Per-version detailed note files:** these keep the shared edit point, now in a different file, and duplicate what commit bodies and design documents already say.
- **A union merge driver:** GitHub ignores it on the server side, and it misorders entries.
- **Agent-written summaries:** these are non-deterministic, and the mapping from commit to note can no longer be checked, so detection cannot regenerate and compare.
- **GitHub's generated release notes:** these depend on the API, labels and GitHub's format. They are not written into `releases.md` or the readme, so there would be two sources.

Generated notes win because they are deterministic and checkable, cannot conflict, and do not repeat the "why": that lives in commit bodies, linked issues and `design/*.md`, one link away from each line.

## Open questions

- Should a commit be able to opt out, for example with a `Release-Note: none` paragraph, or is the maintenance-path rule enough?
- For workspace releases, should each line name the crates it touched, such as `(tomori-scene)`? That would be deterministic, but long for cross-cutting changes.

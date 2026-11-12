# Release Notes

`releases.md` is generated from Git history when a release is prepared, so do not edit it in a pull request. `releases:check` fails a pull request that does.

- **The pull request title is the release note.** A squash merge makes it the commit subject, which becomes one line of the next version's notes, verbatim and linked to the pull request. Write it as a clear, complete sentence about what changed for users, ending in a period.
- **Breaking changes go in the description.** Add a paragraph beginning `Breaking:`, in any case, that says what breaks and how to migrate. Each such paragraph is listed under **Breaking changes** at the top of the version's notes. It may sit anywhere in the description, and a description may have several.
- **Maintenance-only changes appear in no notes.** A commit that touches only `.github/`, `.agents/`, the private `bake/` task package, or a path listed in `[workspace.metadata.bake.release] notes-exclude` is left out.
- **Preview before releasing.** `cargo bake releases:notes Unreleased` prints the next section, and `cargo bake releases:pending` lists the commits it describes. There is nothing to edit: a poor line is fixed by writing clearer titles from then on.

The repository should squash merge only, with the pull request title and description as the commit message, so that each pull request is one line and its `Breaking:` paragraphs reach the commit body.

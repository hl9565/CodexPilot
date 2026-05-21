# Release Notes Design

## Goal

CodexPilot should be able to publish GitHub Releases whose notes include merged pull requests, linked issues, new contributors, and a full changelog link, matching GitHub's native generated release notes behavior.

## Approach

Use GitHub's built-in release notes generator instead of maintaining a custom changelog script. A new manual workflow creates a release from a supplied tag and passes `--generate-notes` to the GitHub CLI. This keeps the release body aligned with GitHub's own compare data and pull request metadata.

The existing `release-assets.yml` workflow remains responsible for packaging and uploading installer artifacts after a release is published. The new workflow only creates the release and does not build assets.

## Release Flow

1. Run the `Create release` workflow manually.
2. Enter a tag such as `v1.0.5`.
3. If the tag already exists, the workflow verifies and releases that tag.
4. If the tag does not exist, GitHub creates the tag at the workflow commit.
5. GitHub generates release notes from merged pull requests and the previous release tag.
6. The existing release asset workflow runs after the release is published.

## Notes Content

Release notes are grouped with `.github/release.yml`:

- Features: `feature`, `enhancement`
- Fixes: `bug`, `fix`
- Documentation: `documentation`, `docs`
- Maintenance: `chore`, `dependencies`, `refactor`
- Other Changes: everything else

Pull requests can close issues by using GitHub keywords such as `Fixes #123` or `Closes #123`. GitHub then shows the pull request in the release notes, and the linked issue remains traceable from the pull request.

## Error Handling

The workflow verifies an existing tag before release creation. If the tag is missing, it creates the release from the workflow commit. GitHub fails the run if the release already exists, if the token lacks permission, or if the repository settings block release creation.

## Testing

Validation is primarily static because publishing a release would mutate the remote repository. The workflow YAML should be checked for syntax and reviewed against GitHub CLI release options. A real test can be done later with a prerelease tag if needed.

## Design Consistency

No existing release-notes design document was present. This spec defines the new behavior, and the implementation follows it directly.

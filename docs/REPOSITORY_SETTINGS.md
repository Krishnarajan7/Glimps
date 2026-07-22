# Required GitHub Repository Settings

These controls cannot be committed as repository files. Apply them after the
Phase 3 commit is green, and review them before every public release.

## Private Security Reporting

In repository **Settings → Security**, enable private vulnerability reporting.
Confirm that **Security → Report a vulnerability** is visible to a signed-in
non-maintainer. This activates the private path documented in `SECURITY.md`.

Also enable Dependabot alerts and security updates. The committed
`.github/dependabot.yml` handles routine version-update pull requests.

## Main Branch Ruleset

Create an active branch ruleset targeting `main`:

- block branch deletion and force pushes;
- require pull requests for changes;
- require conversations to be resolved;
- require the branch to be up to date before merging;
- require these status checks:
  - `test (ubuntu-24.04)`
  - `test (macos-14)`
  - `security audit`
  - `site quality`
- require signed commits when every maintainer machine and automation identity is
  ready for it; and
- do not permit broad “always bypass” access.

GLIMPS currently has one maintainer. Do not require a CODEOWNER approval until a
second trusted reviewer exists, because GitHub does not count self-approval.
CODEOWNERS still documents the trust boundaries and will become enforceable when
the reviewer pool grows.

## Release And Tag Protection

Create a tag ruleset for `v*` that blocks deletion and non-fast-forward updates.
Limit tag creation to maintainers, use signed annotated tags, and configure a
`release` environment with required maintainer approval before public publishing.

The release workflow itself defaults to read-only. Its `host` job alone receives
`contents: write`, `id-token: write`, and `attestations: write`. The Homebrew job
uses only the dedicated `HOMEBREW_TAP_TOKEN`; scope that token to the tap
repository with contents write and no broader account permissions.

## Actions Policy

Set the repository's default workflow token permission to **read repository
contents**. Allow GitHub-authored actions plus only the third-party actions named
and SHA-pinned in the committed workflows. Require approval for workflows first
introduced by outside contributors.

After saving the settings, open a small documentation pull request and confirm
that direct pushes are blocked, all four checks are required, and the maintainer
can merge through the intended reviewed path.

# Security Policy

GLIMPS supervises a shell and processes terminal output, so reports involving
terminal restoration, command metadata, secret exposure, process cleanup, or
release artifacts are treated as security-sensitive.

## Supported Versions

GLIMPS is currently a pre-release project. Security fixes are made on `main`.
After public releases begin, only the latest release in the newest minor series
will receive security fixes unless a release note says otherwise.

## Report a Vulnerability Privately

Use GitHub's **Security → Report a vulnerability** form for this repository.
Please do not disclose suspected vulnerabilities in public issues, pull
requests, discussions, screenshots, or terminal recordings.

If private vulnerability reporting is temporarily unavailable, open a public
issue containing only a request for private maintainer contact. Do not include
technical details, terminal output, credentials, paths, or reproduction steps.

Include only what is necessary:

- the affected commit or version and operating system;
- the security impact and prerequisites;
- minimal reproduction steps using synthetic data;
- whether the issue is already public; and
- a safe way to contact you for follow-up.

Never attach real tokens, passwords, private keys, production output, or an
unredacted shell configuration.

## Response Targets

The maintainer will make a best effort to:

- acknowledge a report within 72 hours;
- provide an initial severity assessment within 7 days;
- send progress updates at least every 14 days while remediation is active; and
- coordinate disclosure after a fix or mitigation is available.

These are response targets, not a bug-bounty or compensation commitment.

## Scope Priorities

High-priority reports include:

- terminal state not being restored after exit or signals;
- command or output bytes being altered in pass-through paths;
- secrets being captured, persisted, displayed in GLIMPS-authored summaries, or
  transmitted;
- forged terminal data being presented as trusted GLIMPS metadata;
- orphaned shell or foreground processes;
- arbitrary code execution or unsafe file permissions; and
- compromised or unverifiable release artifacts and installers.

General bugs, feature requests, and expected formatter false positives belong in
the public issue tracker, provided they contain no sensitive terminal data.

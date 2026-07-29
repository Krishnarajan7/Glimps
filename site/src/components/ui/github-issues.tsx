import { useEffect, useState } from "react";

const ISSUES_URL = "https://github.com/Krishnarajan7/Glimps/issues";
const API_URL =
  "https://api.github.com/repos/Krishnarajan7/Glimps/issues?state=open&per_page=10";
const CACHE_KEY = "glimps:github-issues";
const CACHE_TTL_MS = 60 * 60 * 1000;

export type Issue = {
  number: number;
  title: string;
  url: string;
  labels: string[];
};

type CachedIssues = {
  issues: Issue[];
  savedAt: number;
};

function isIssue(value: unknown): value is Issue {
  const v = value as Issue;
  return (
    typeof v === "object" &&
    v !== null &&
    typeof v.number === "number" &&
    typeof v.title === "string" &&
    typeof v.url === "string" &&
    Array.isArray(v.labels) &&
    v.labels.every((l) => typeof l === "string")
  );
}

function readCachedIssues(): Issue[] | null {
  try {
    const raw = window.sessionStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const cached = JSON.parse(raw) as CachedIssues;
    if (
      !Array.isArray(cached.issues) ||
      !cached.issues.every(isIssue) ||
      Date.now() - cached.savedAt > CACHE_TTL_MS
    ) {
      return null;
    }
    return cached.issues;
  } catch {
    return null;
  }
}

function saveCachedIssues(issues: Issue[]) {
  try {
    window.sessionStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ issues, savedAt: Date.now() } satisfies CachedIssues),
    );
  } catch {
    // Storage can be unavailable in private or locked-down browsing modes.
  }
}

/* A few label names get a semantic color from the site palette; the rest
   stay dim. Chip colors are ours, not GitHub's, so they fit both themes. */
function labelColor(name: string): string {
  if (name === "good first issue") return "var(--color-syn-string)";
  if (name === "help wanted") return "var(--color-syn-key)";
  if (name === "safety") return "var(--color-syn-error)";
  return "var(--color-syn-dim)";
}

/**
 * Live list of open issues, fetched client-side from the GitHub API (cached
 * for an hour per tab). On failure it renders nothing — callers keep a plain
 * link to the issues page next to this component, so GitHub being down or
 * rate-limited never hides the way in.
 */
export function GitHubIssues() {
  const [issues, setIssues] = useState<Issue[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const cached = readCachedIssues();
    if (cached !== null) {
      setIssues(cached);
      return;
    }

    const controller = new AbortController();
    fetch(API_URL, {
      headers: { Accept: "application/vnd.github+json" },
      signal: controller.signal,
    })
      .then((response) => {
        if (!response.ok) throw new Error(`GitHub API returned ${response.status}`);
        return response.json() as Promise<unknown>;
      })
      .then((data) => {
        if (!Array.isArray(data)) throw new Error("unexpected payload");
        const parsed = data
          // The issues endpoint also returns pull requests — drop them.
          .filter((item) => !(item as { pull_request?: unknown }).pull_request)
          .map((item) => {
            const it = item as {
              number?: unknown;
              title?: unknown;
              html_url?: unknown;
              labels?: unknown;
            };
            return {
              number: typeof it.number === "number" ? it.number : -1,
              title: typeof it.title === "string" ? it.title : "",
              url: typeof it.html_url === "string" ? it.html_url : ISSUES_URL,
              labels: Array.isArray(it.labels)
                ? it.labels
                    .map((l) => (l as { name?: unknown }).name)
                    .filter((n): n is string => typeof n === "string")
                : [],
            };
          })
          .filter((it) => it.number > 0 && it.title.length > 0);
        setIssues(parsed);
        saveCachedIssues(parsed);
      })
      .catch(() => setFailed(true));

    return () => controller.abort();
  }, []);

  if (failed || (issues !== null && issues.length === 0)) return null;

  if (issues === null) {
    return (
      <p className="font-mono text-sm text-muted-foreground" role="status">
        fetching open issues from GitHub…
      </p>
    );
  }

  return (
    <ul className="space-y-2">
      {issues.map((issue) => (
        <li key={issue.number}>
          <a
            href={issue.url}
            target="_blank"
            rel="noopener noreferrer"
            className="group flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded border px-4 py-2.5 hover:bg-muted/60 transition-colors"
            style={{ borderColor: "var(--color-border)" }}
          >
            <span className="font-mono text-xs text-[var(--color-syn-dim)] tabular-nums">
              #{issue.number}
            </span>
            <span className="font-mono text-sm text-foreground/90 group-hover:text-foreground">
              {issue.title}
            </span>
            {issue.labels.map((label) => (
              <span
                key={label}
                className="rounded border px-1.5 py-0.5 font-mono text-[10px] leading-none"
                style={{
                  borderColor: "var(--color-border)",
                  color: labelColor(label),
                }}
              >
                {label}
              </span>
            ))}
          </a>
        </li>
      ))}
    </ul>
  );
}

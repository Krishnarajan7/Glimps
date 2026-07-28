import { useEffect, useState } from "react";

const REPOSITORY_URL = "https://github.com/Krishnarajan7/Glimps";
const API_URL = "https://api.github.com/repos/Krishnarajan7/Glimps";
const CACHE_KEY = "glimps:github-stars";
const CACHE_TTL_MS = 60 * 60 * 1000;

type CachedStars = {
  count: number;
  savedAt: number;
};

function readCachedStars(): number | null {
  try {
    const raw = window.sessionStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const cached = JSON.parse(raw) as CachedStars;
    if (
      !Number.isInteger(cached.count) ||
      cached.count < 0 ||
      Date.now() - cached.savedAt > CACHE_TTL_MS
    ) {
      return null;
    }
    return cached.count;
  } catch {
    return null;
  }
}

function saveCachedStars(count: number) {
  try {
    window.sessionStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ count, savedAt: Date.now() } satisfies CachedStars),
    );
  } catch {
    // Storage can be unavailable in private or locked-down browsing modes.
  }
}

function formatStars(count: number) {
  return new Intl.NumberFormat("en", {
    notation: count >= 1000 ? "compact" : "standard",
    maximumFractionDigits: 1,
  }).format(count);
}

export function GitHubStars({ className = "" }: { className?: string }) {
  const [stars, setStars] = useState<number | null>(null);

  useEffect(() => {
    const cached = readCachedStars();
    if (cached !== null) {
      setStars(cached);
      return;
    }

    const controller = new AbortController();
    fetch(API_URL, {
      headers: { Accept: "application/vnd.github+json" },
      signal: controller.signal,
    })
      .then((response) => {
        if (!response.ok) throw new Error(`GitHub API returned ${response.status}`);
        return response.json() as Promise<{ stargazers_count?: unknown }>;
      })
      .then((repository) => {
        if (
          typeof repository.stargazers_count !== "number" ||
          !Number.isInteger(repository.stargazers_count)
        ) {
          return;
        }
        setStars(repository.stargazers_count);
        saveCachedStars(repository.stargazers_count);
      })
      .catch(() => {
        // The repository link remains usable when GitHub is unavailable or rate-limited.
      });

    return () => controller.abort();
  }, []);

  const label =
    stars === null
      ? "View GLIMPS on GitHub"
      : `View GLIMPS on GitHub — ${stars} ${stars === 1 ? "star" : "stars"}`;

  return (
    <a
      href={REPOSITORY_URL}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={label}
      title={label}
      className={
        "items-center gap-1.5 rounded border px-2.5 py-1.5 text-xs text-muted-foreground " +
        "hover:bg-muted hover:text-foreground transition-colors " +
        className
      }
      style={{ borderColor: "var(--color-border)" }}
    >
      <span>github</span>
      <span className="text-[var(--color-syn-number)]" aria-hidden="true">
        ★
      </span>
      <span className="min-w-[1ch] tabular-nums" aria-live="polite">
        {stars === null ? "—" : formatStars(stars)}
      </span>
    </a>
  );
}

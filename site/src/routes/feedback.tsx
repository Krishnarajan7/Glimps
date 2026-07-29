import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import {
  DocsLayout,
  H2,
  H3,
  P,
  UL,
  Callout,
  Code,
  type TocItem,
} from "../components/DocsLayout";
import { canonical } from "@/lib/seo";
import { Glimps } from "@/components/ui/glimps";
import { GitHubIssues } from "@/components/ui/github-issues";

export const Route = createFileRoute("/feedback")({
  head: () => ({
    meta: [
      { title: "Feedback — GLIMPS" },
      {
        name: "description",
        content:
          "Rate GLIMPS out of 5 stars, send feedback straight to the maintainer, or go further — pick up a good first issue and become a contributor.",
      },
      { property: "og:title", content: "GLIMPS feedback" },
      {
        property: "og:description",
        content:
          "Rate GLIMPS out of 5 stars and send feedback straight to the maintainer.",
      },
      { property: "og:url", content: canonical("/feedback") },
    ],
    links: [{ rel: "canonical", href: canonical("/feedback") }],
  }),
  component: FeedbackPage,
});

const toc: TocItem[] = [
  { id: "rating", label: "Rating" },
  { id: "send", label: "Send feedback" },
  { id: "reviews", label: "Reviews from this device" },
  { id: "contribute", label: "Become a contributor" },
  { id: "open-issues", label: "Open issues", depth: 2 },
  { id: "privacy", label: "Where it goes" },
];

/** The address every submission is delivered to. */
const FEEDBACK_EMAIL = "krishh.v777@gmail.com";
const FORM_ENDPOINT = `https://formsubmit.co/ajax/${FEEDBACK_EMAIL}`;
const STORAGE_KEY = "glimps-feedback-reviews";

type Review = {
  name: string;
  rating: number; // 1–5
  message: string;
  date: string; // ISO
};

function loadReviews(): Review[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (r): r is Review =>
        typeof r === "object" &&
        r !== null &&
        typeof (r as Review).rating === "number" &&
        typeof (r as Review).message === "string",
    );
  } catch {
    return [];
  }
}

function saveReviews(reviews: Review[]) {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(reviews));
  } catch {
    // Storage full or blocked — the review was still emailed, so drop silently.
  }
}

/* Five stars with a fractional fill — the dim row underneath is the track,
   the colored row above is clipped to the average. */
function Stars({
  value,
  className = "text-2xl",
}: {
  value: number;
  className?: string;
}) {
  const pct = (Math.max(0, Math.min(5, value)) / 5) * 100;
  return (
    <span
      aria-hidden="true"
      className={`relative inline-block font-mono leading-none align-middle ${className}`}
    >
      <span className="text-[var(--color-syn-dim)]">★★★★★</span>
      <span
        className="absolute inset-y-0 left-0 overflow-hidden whitespace-nowrap text-[var(--color-syn-number)]"
        style={{ width: `${pct}%` }}
      >
        ★★★★★
      </span>
    </span>
  );
}

/* Star picker for the form — real radio inputs (keyboard + screen-reader
   friendly), stars drawn on the labels with a hover preview. */
function StarPicker({
  rating,
  onChange,
}: {
  rating: number;
  onChange: (r: number) => void;
}) {
  const [hover, setHover] = useState(0);
  const shown = hover || rating;
  return (
    <fieldset className="border-0 p-0 m-0">
      <legend className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-2">
        Your rating (out of 5)
      </legend>
      <div className="flex items-center gap-1" onMouseLeave={() => setHover(0)}>
        {[1, 2, 3, 4, 5].map((n) => (
          <label
            key={n}
            className="cursor-pointer select-none px-0.5"
            onMouseEnter={() => setHover(n)}
          >
            <input
              type="radio"
              name="rating"
              value={n}
              checked={rating === n}
              onChange={() => onChange(n)}
              className="sr-only"
              aria-label={`${n} star${n === 1 ? "" : "s"}`}
            />
            <span
              aria-hidden="true"
              className={
                "font-mono text-3xl leading-none transition-colors " +
                (n <= shown
                  ? "text-[var(--color-syn-number)]"
                  : "text-[var(--color-syn-dim)]")
              }
            >
              {n <= shown ? "★" : "☆"}
            </span>
          </label>
        ))}
        <span className="ml-3 font-mono text-sm text-muted-foreground">
          {rating > 0 ? `${rating} / 5` : "pick one"}
        </span>
      </div>
    </fieldset>
  );
}

const inputClass =
  "w-full rounded border bg-transparent px-3 py-2 font-mono text-sm " +
  "placeholder:text-muted-foreground/60 focus:outline-none focus:ring-2 " +
  "focus:ring-[var(--color-ring)]";

function FeedbackPage() {
  const [reviews, setReviews] = useState<Review[]>([]);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [rating, setRating] = useState(0);
  const [message, setMessage] = useState("");
  const [status, setStatus] = useState<"idle" | "sending" | "sent" | "error">(
    "idle",
  );

  useEffect(() => {
    setReviews(loadReviews());
  }, []);

  const average = useMemo(() => {
    if (!reviews.length) return 0;
    return reviews.reduce((sum, r) => sum + r.rating, 0) / reviews.length;
  }, [reviews]);

  const canSubmit =
    rating > 0 && message.trim().length > 0 && status !== "sending";

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setStatus("sending");
    try {
      const res = await fetch(FORM_ENDPOINT, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "application/json",
        },
        body: JSON.stringify({
          name: name.trim() || "Anonymous",
          email: email.trim() || "not provided",
          rating: `${rating} / 5`,
          message: message.trim(),
          _subject: `GLIMPS feedback — ${rating}/5 stars`,
          _template: "table",
          _captcha: "false",
        }),
      });
      if (!res.ok) throw new Error(`formsubmit responded ${res.status}`);
      const next = [
        {
          name: name.trim() || "Anonymous",
          rating,
          message: message.trim(),
          date: new Date().toISOString(),
        },
        ...reviews,
      ];
      setReviews(next);
      saveReviews(next);
      setName("");
      setEmail("");
      setRating(0);
      setMessage("");
      setStatus("sent");
    } catch {
      setStatus("error");
    }
  };

  return (
    <DocsLayout
      section="Community"
      title="Feedback"
      intro={
        <>
          <Glimps quiet /> is built in the open, and reviews are how it gets
          better. Rate it out of 5, say what worked and what broke — every
          submission lands directly in the maintainer's inbox. And if you'd
          rather fix things than describe them,{" "}
          <a href="#contribute" className="underline">
            become a contributor
          </a>
          .
        </>
      }
      toc={toc}
    >
      <section className="space-y-4">
        <H2 id="rating">Rating</H2>
        {reviews.length > 0 ? (
          <div
            className="rounded-lg border overflow-hidden bg-[var(--color-terminal-bg)]"
            style={{ borderColor: "var(--color-terminal-border)" }}
          >
            <div className="px-4 py-4 font-mono">
              <div className="text-xs text-[var(--color-syn-dim)] mb-2">
                <span className="text-[var(--color-bar)]">▌ </span>
                glimps feedback --stats
              </div>
              <div className="flex flex-wrap items-center gap-3">
                <Stars value={average} />
                <span className="text-lg font-semibold">
                  {Number.isInteger(average) ? average : average.toFixed(1)} / 5
                </span>
                <span className="text-sm text-[var(--color-syn-dim)]">
                  from {reviews.length} review
                  {reviews.length === 1 ? "" : "s"} on this device
                </span>
              </div>
            </div>
          </div>
        ) : (
          <P>
            No reviews from this browser yet — yours would be the first. The
            average here is calculated out of 5 from the reviews sent on this
            device.
          </P>
        )}
      </section>

      <section className="space-y-4">
        <H2 id="send">Send feedback</H2>
        <form onSubmit={onSubmit} className="space-y-5 max-w-xl">
          <StarPicker rating={rating} onChange={setRating} />

          <div className="grid sm:grid-cols-2 gap-4">
            <div>
              <label
                htmlFor="fb-name"
                className="block font-mono text-xs uppercase tracking-widest text-muted-foreground mb-2"
              >
                Name <span className="normal-case">(optional)</span>
              </label>
              <input
                id="fb-name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Ada"
                maxLength={80}
                className={inputClass}
                style={{ borderColor: "var(--color-border)" }}
              />
            </div>
            <div>
              <label
                htmlFor="fb-email"
                className="block font-mono text-xs uppercase tracking-widest text-muted-foreground mb-2"
              >
                Email <span className="normal-case">(optional, for replies)</span>
              </label>
              <input
                id="fb-email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@example.com"
                maxLength={120}
                className={inputClass}
                style={{ borderColor: "var(--color-border)" }}
              />
            </div>
          </div>

          <div>
            <label
              htmlFor="fb-message"
              className="block font-mono text-xs uppercase tracking-widest text-muted-foreground mb-2"
            >
              Your review
            </label>
            <textarea
              id="fb-message"
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="What worked, what broke, what's missing…"
              rows={5}
              maxLength={2000}
              required
              className={inputClass + " resize-y"}
              style={{ borderColor: "var(--color-border)" }}
            />
          </div>

          <div className="flex flex-wrap items-center gap-4">
            <button
              type="submit"
              disabled={!canSubmit}
              className={
                "rounded px-4 py-2 font-mono text-sm font-semibold transition-colors " +
                (canSubmit
                  ? "bg-[var(--color-foreground)] text-[var(--color-background)] hover:opacity-90"
                  : "bg-muted text-muted-foreground cursor-not-allowed")
              }
            >
              {status === "sending" ? "sending…" : "send review"}
            </button>
            <span
              role="status"
              aria-live="polite"
              className="font-mono text-sm"
            >
              {status === "sent" && (
                <span className="text-[var(--color-syn-string)]">
                  ✓ sent — thank you!
                </span>
              )}
              {status === "error" && (
                <span className="text-[var(--color-syn-error)]">
                  ✗ couldn't send — check your connection and try again
                </span>
              )}
              {status !== "sent" && status !== "error" && rating === 0 && (
                <span className="text-muted-foreground">
                  pick a star rating to enable send
                </span>
              )}
            </span>
          </div>
        </form>
      </section>

      <section className="space-y-4">
        <H2 id="reviews">Reviews from this device</H2>
        {reviews.length === 0 ? (
          <P>Reviews you send from this browser will show up here.</P>
        ) : (
          <ul className="space-y-3">
            {reviews.map((r, i) => (
              <li
                key={`${r.date}-${i}`}
                className="rounded-lg border px-4 py-3"
                style={{ borderColor: "var(--color-border)" }}
              >
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 mb-1.5">
                  <Stars value={r.rating} className="text-base" />
                  <span className="font-mono text-sm font-semibold">
                    {r.name || "Anonymous"}
                  </span>
                  <span className="font-mono text-xs text-muted-foreground">
                    {new Date(r.date).toLocaleDateString()}
                  </span>
                </div>
                <p className="text-[14px] leading-6 text-foreground/90 whitespace-pre-wrap">
                  {r.message}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="space-y-4">
        <H2 id="contribute">Become a contributor</H2>
        <P>
          Reviews shape <Glimps quiet />; code moves it. The project is a Rust
          codebase built in the open, and it's deliberately structured so a
          newcomer can land a real change: most open work is adding coloring
          for a specific command's output, and every formatter follows the same
          established pattern — a detector, a formatter, golden-file tests, and
          a property test proving byte-safety.
        </P>
        <UL>
          <li>
            Start at{" "}
            <a
              href="https://github.com/Krishnarajan7/Glimps/issues/1"
              target="_blank"
              rel="noopener noreferrer"
              className="underline"
            >
              issue #1 — "Start contributing to GLIMPS"
            </a>
            , the guided entry point for first-time contributors.
          </li>
          <li>
            Read{" "}
            <a
              href="https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md"
              target="_blank"
              rel="noopener noreferrer"
              className="underline"
            >
              CONTRIBUTING.md
            </a>{" "}
            for the dev setup, the safety invariants, and what a PR needs to
            pass review.
          </li>
          <li>
            Pick an issue below, comment on it to claim it, and open a PR —
            small, focused changes are preferred over big ones.
          </li>
        </UL>
        <Callout title="safety first">
          <Glimps quiet /> sits between you and everything your terminal shows,
          so issues labeled <Code>safety</Code> touch a hard invariant: never
          corrupt the terminal, never drop or reorder bytes. The tests enforce
          this — the pattern to follow is already in the codebase.
        </Callout>

        <H3 id="open-issues">Open issues</H3>
        <P>
          These are live from GitHub — every one of them is currently labeled{" "}
          <Code>good first issue</Code>. The full list is on the{" "}
          <a
            href="https://github.com/Krishnarajan7/Glimps/issues"
            target="_blank"
            rel="noopener noreferrer"
            className="underline"
          >
            issue tracker
          </a>
          .
        </P>
        <GitHubIssues />
      </section>

      <section className="space-y-4">
        <H2 id="privacy">Where it goes</H2>
        <P>
          This site has no backend, so the form hands your submission to{" "}
          <a href="https://formsubmit.co" className="underline">
            FormSubmit
          </a>
          , which emails it to the maintainer at <Code>{FEEDBACK_EMAIL}</Code>.
          Only what you type in the form is sent — nothing is collected in the
          background, and the same no-telemetry promise as the product applies.
        </P>
        <Callout title="local only">
          The star average and review list above live in this browser's{" "}
          <Code>localStorage</Code> — they reflect reviews sent from this
          device, not a global score. Clearing site data resets them; the
          emails already sent are unaffected.
        </Callout>
      </section>
    </DocsLayout>
  );
}

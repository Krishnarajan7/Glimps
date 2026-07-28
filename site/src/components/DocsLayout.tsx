import { Link, useRouterState } from "@tanstack/react-router";
import { GitHubStars } from "@/components/ui/github-stars";
import { GlimpsMark } from "@/components/ui/glimps-mark";
import {
  useEffect,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
} from "react";

export type TocItem = { id: string; label: string; depth?: 1 | 2 };

/* The product name in nav labels, set in the palette's "key" cyan — the
   same color the .glimps-word prose treatment uses. */
function NavGlimps() {
  return (
    <span className="text-[oklch(0.47_0.12_210)] dark:text-[var(--color-syn-key)]">
      GLIMPS
    </span>
  );
}

const NAV: { group: string; items: { to: string; label: ReactNode }[] }[] = [
  {
    group: "",
    items: [
      {
        to: "/",
        label: (
          <>
            <NavGlimps /> Home
          </>
        ),
      },
      {
        to: "/about",
        label: (
          <>
            About <NavGlimps />
          </>
        ),
      },
    ],
  },
  {
    group: "Install",
    items: [{ to: "/installation", label: "Installation" }],
  },
  {
    group: "Reference",
    items: [{ to: "/features", label: "Features" }],
  },
];

/* Nav link groups, shared by the desktop sidebar and the mobile sheet.
   The active page carries the ▌ bar — the same mark GLIMPS paints at the
   start of a command — so "you are here" reads in the product's own glyph. */
function NavGroups({
  pathname,
  variant,
  onNavigate,
}: {
  pathname: string;
  variant: "sidebar" | "sheet";
  onNavigate?: () => void;
}) {
  const sheet = variant === "sheet";
  return (
    <div className={sheet ? "space-y-5" : "space-y-6"}>
      {NAV.map((g) => (
        <div key={g.group || "root"}>
          {g.group && (
            <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-2 px-3">
              {g.group}
            </div>
          )}
          <ul className="space-y-0.5">
            {g.items.map((it) => {
              const isActive = pathname === it.to;
              return (
                <li key={it.to}>
                  <Link
                    to={it.to}
                    onClick={onNavigate}
                    aria-current={isActive ? "page" : undefined}
                    className={
                      "flex items-center rounded px-3 transition-colors " +
                      (sheet ? "py-2.5 " : "py-1.5 ") +
                      (isActive
                        ? "bg-muted text-foreground"
                        : "text-muted-foreground hover:text-foreground hover:bg-muted/60")
                    }
                  >
                    <span
                      aria-hidden="true"
                      className={
                        "mr-2 inline-block w-[1ch] shrink-0 text-[var(--color-bar)]" +
                        (isActive ? "" : " opacity-0")
                      }
                    >
                      ▌
                    </span>
                    {it.label}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </div>
  );
}

function ThemeButton() {
  const [theme, setTheme] = useState<"light" | "dark">(() =>
    typeof document !== "undefined" && document.documentElement.classList.contains("dark")
      ? "dark"
      : "light",
  );
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);
  return (
    <button
      onClick={() => setTheme((t) => (t === "light" ? "dark" : "light"))}
      aria-label="Toggle theme"
      className="px-2.5 py-1.5 rounded border text-xs font-mono hover:bg-muted transition-colors"
      style={{ borderColor: "var(--color-border)" }}
    >
      {theme === "dark" ? "◐ light" : "◑ dark"}
    </button>
  );
}

export function DocsLayout({
  section,
  title,
  intro,
  toc,
  children,
}: {
  section: string;
  title: string;
  intro?: ReactNode;
  toc: TocItem[];
  children: ReactNode;
}) {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const [active, setActive] = useState<string>(toc[0]?.id ?? "");
  const [navOpen, setNavOpen] = useState(false);
  const mainRef = useRef<HTMLElement>(null);
  const sheetRef = useRef<HTMLDivElement>(null);
  const menuButtonRef = useRef<HTMLButtonElement>(null);

  // The mobile sheet overlays the page: lock the page scroll behind it,
  // close on Escape (returning focus to the toggle), and move focus into
  // the sheet so keyboard/screen-reader users land where the action is.
  useEffect(() => {
    if (!navOpen) return;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setNavOpen(false);
        menuButtonRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    sheetRef.current?.focus({ preventScroll: true });
    return () => {
      document.body.style.overflow = prevOverflow;
      window.removeEventListener("keydown", onKey);
    };
  }, [navOpen]);

  // If the viewport crosses into desktop while the sheet is open, the sheet
  // is display:none but the body scroll lock would linger — release it.
  useEffect(() => {
    if (typeof window === "undefined") return;
    const mq = window.matchMedia("(min-width: 1024px)");
    const onChange = () => {
      if (mq.matches) setNavOpen(false);
    };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    setNavOpen(false);
  }, [pathname]);

  // Only <main> scrolls on desktop, so a route change won't reset it the way a
  // normal page navigation resets the window. Scroll the article back to the top
  // on every path change (and the window, for the mobile single-column layout).
  useEffect(() => {
    // Instant, not smooth — a route change should start at the top immediately,
    // overriding the smooth scroll-behavior used for in-page anchor clicks.
    mainRef.current?.scrollTo({ top: 0, behavior: "instant" });
    if (typeof window !== "undefined")
      window.scrollTo({ top: 0, behavior: "instant" });
  }, [pathname]);

  // Smoothly scroll a heading into view when its "On this page" link is clicked.
  // We scroll the actual container directly (computing the target position)
  // rather than using scrollIntoView — the latter is flaky on the first click
  // inside a container that already has `scroll-behavior: smooth`.
  const scrollToId = (id: string) => {
    const el = document.getElementById(id);
    if (!el) return;
    const reduce =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const behavior: ScrollBehavior = reduce ? "auto" : "smooth";
    const main = mainRef.current;
    if (main && main.scrollHeight > main.clientHeight + 1) {
      const offset = 24; // small breathing room above the heading
      const top =
        el.getBoundingClientRect().top -
        main.getBoundingClientRect().top +
        main.scrollTop -
        offset;
      main.scrollTo({ top: Math.max(0, top), behavior });
    } else if (typeof window !== "undefined") {
      // Window scrolling means the mobile layout, where the sticky h-14
      // header overlays the top — clear it, plus the same breathing room.
      const offset = 80;
      const top = el.getBoundingClientRect().top + window.scrollY - offset;
      window.scrollTo({ top: Math.max(0, top), behavior });
    }
    setActive(id);
  };

  const scrollToHeading = (
    e: ReactMouseEvent<HTMLAnchorElement>,
    id: string,
  ) => {
    e.preventDefault();
    scrollToId(id);
  };

  // From the mobile sheet, close first and scroll on the next frames — the
  // body scroll lock must be released before the target position is computed.
  const sheetScrollToHeading = (
    e: ReactMouseEvent<HTMLAnchorElement>,
    id: string,
  ) => {
    e.preventDefault();
    setNavOpen(false);
    requestAnimationFrame(() => requestAnimationFrame(() => scrollToId(id)));
  };

  // Scroll-spy for the "On this page" list. The scroll container is <main> on
  // desktop and the window on mobile. A section normally becomes active when its
  // heading scrolls up past a band near the top — but the last few sections are
  // often too short to ever push their heading to that band before the page
  // bottoms out. So compute each heading's activation scroll position and spread
  // any unreachable tail across the remaining scroll, giving every section a
  // window and snapping to the last one at the true bottom.
  useEffect(() => {
    if (typeof window === "undefined") return;

    const compute = () => {
      const main = mainRef.current;
      const scrollsInMain = !!main && main.scrollHeight > main.clientHeight + 1;
      const scrollTop = scrollsInMain ? main.scrollTop : window.scrollY;
      const viewport = scrollsInMain ? main.clientHeight : window.innerHeight;
      const scrollHeight = scrollsInMain
        ? main.scrollHeight
        : document.documentElement.scrollHeight;
      const maxScroll = Math.max(0, scrollHeight - viewport);
      const baseTop = scrollsInMain ? main.getBoundingClientRect().top : 0;

      // Headings that actually exist on the page, in document order.
      const items = toc
        .map((t) => document.getElementById(t.id))
        .filter((el): el is HTMLElement => !!el)
        .map((el) => ({
          id: el.id,
          // offset of the heading from the top of the scrollable content
          offset: el.getBoundingClientRect().top - baseTop + scrollTop,
        }));
      if (!items.length) return;

      const threshold = 96;
      const acts = items.map((it) => Math.max(0, it.offset - threshold));

      // Trailing sections are often too short to ever push their heading to the
      // top band before the page bottoms out, so they'd all activate only at the
      // very bottom (skipping). Find the smallest group of crowded trailing
      // headings that can't each get a comfortable scroll window at their natural
      // positions, then spread exactly that group across the scroll that remains
      // before them — so every section gets a real, non-vanishing window.
      const perItem = 72; // desired scroll distance each heading stays active
      let tailStart = acts.length;
      for (let i = acts.length - 1; i >= 0; i--) {
        const count = acts.length - i;
        const anchor = i > 0 ? acts[i - 1] : 0;
        tailStart = i;
        if (maxScroll - anchor >= count * perItem) break;
      }
      if (tailStart < acts.length) {
        const anchor = tailStart > 0 ? acts[tailStart - 1] : 0;
        const span = Math.max(0, maxScroll - anchor);
        const count = acts.length - tailStart;
        for (let k = 0; k < count; k++) {
          acts[tailStart + k] = anchor + (span * (k + 1)) / (count + 1);
        }
      }

      let idx = 0;
      for (let i = 0; i < acts.length; i++) {
        if (scrollTop >= acts[i]) idx = i;
        else break;
      }
      if (scrollTop >= maxScroll - 2) idx = acts.length - 1; // true bottom
      setActive(items[idx].id);
    };

    compute();
    const main = mainRef.current;
    main?.addEventListener("scroll", compute, { passive: true });
    window.addEventListener("scroll", compute, { passive: true });
    window.addEventListener("resize", compute);
    return () => {
      main?.removeEventListener("scroll", compute);
      window.removeEventListener("scroll", compute);
      window.removeEventListener("resize", compute);
    };
  }, [toc, pathname]);

  return (
    <div className="relative min-h-screen lg:h-screen lg:flex lg:flex-col lg:overflow-hidden">
      {/* Header */}
      <header
        className="sticky top-0 z-30 border-b backdrop-blur bg-[var(--color-background)]/85 lg:shrink-0"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="mx-auto max-w-7xl px-4 sm:px-6 h-14 flex items-center gap-4">
          <Link to="/" className="flex items-center gap-2 font-mono font-semibold min-w-0">
            <GlimpsMark size={22} className="shrink-0" />
            <span className="truncate">glimps</span>
            <span
              className="ml-1 shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
              style={{ background: "var(--color-muted)", color: "var(--color-muted-foreground)" }}
            >
              docs
            </span>
          </Link>
          <div className="ml-auto flex items-center gap-2 font-mono text-sm">
            <Link
              to="/"
              className="hidden sm:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
            >
              landing
            </Link>
            <GitHubStars className="hidden sm:flex" />
            <ThemeButton />
            <button
              ref={menuButtonRef}
              onClick={() => setNavOpen((v) => !v)}
              className={
                "lg:hidden px-2.5 py-1.5 rounded border text-xs transition-colors " +
                (navOpen ? "bg-muted" : "hover:bg-muted")
              }
              style={{ borderColor: "var(--color-border)" }}
              aria-expanded={navOpen}
              aria-controls="mobile-nav"
            >
              <span className="inline-block min-w-[5ch] text-center">
                {navOpen ? "close" : "menu"}
              </span>
            </button>
          </div>
        </div>
      </header>

      {/* Mobile nav sheet — drops from under the header over a scrim instead
          of pushing the article down. Unmounts on close. */}
      {navOpen && (
        <div className="lg:hidden fixed inset-x-0 top-14 bottom-0 z-40">
          <button
            type="button"
            className="nav-scrim absolute inset-0 w-full cursor-default"
            onClick={() => setNavOpen(false)}
            aria-label="Close navigation"
            tabIndex={-1}
          />
          <div
            id="mobile-nav"
            ref={sheetRef}
            tabIndex={-1}
            className="nav-sheet relative max-h-full overflow-y-auto overscroll-contain border-b bg-[var(--color-background)] outline-none"
            style={{ borderColor: "var(--color-border)" }}
          >
            <nav
              aria-label="GLIMPS docs"
              className="px-4 sm:px-6 pt-5 pb-4 font-mono text-sm"
            >
              <NavGroups
                pathname={pathname}
                variant="sheet"
                onNavigate={() => setNavOpen(false)}
              />
            </nav>
            {toc.length > 0 && (
              <div
                className="border-t px-4 sm:px-6 pt-4 pb-5"
                style={{ borderColor: "var(--color-border)" }}
              >
                <div className="font-mono text-[10px] uppercase tracking-widest text-muted-foreground mb-2 px-3">
                  On this page
                </div>
                <ul className="font-mono text-sm">
                  {toc.map((t) => (
                    <li key={t.id}>
                      <a
                        href={`#${t.id}`}
                        onClick={(e) => sheetScrollToHeading(e, t.id)}
                        className={
                          "block rounded px-3 py-2 text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors" +
                          (t.depth === 2 ? " pl-7 text-xs" : "")
                        }
                      >
                        {t.label}
                      </a>
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <div
              className="border-t px-4 sm:px-6 py-4 flex items-center justify-between font-mono text-sm"
              style={{ borderColor: "var(--color-border)" }}
            >
              <GitHubStars className="flex" />
              <span className="text-xs text-muted-foreground">docs v0.1</span>
            </div>
          </div>
        </div>
      )}

      <div className="lg:flex-1 lg:min-h-0">
        <div className="mx-auto max-w-7xl h-full px-4 sm:px-6 grid grid-cols-1 lg:grid-cols-[240px_minmax(0,1fr)_220px] gap-8 lg:gap-10">
        {/* Left nav (desktop only — mobile uses the sheet above) */}
        <aside className="no-scrollbar hidden lg:block lg:h-full lg:overflow-y-auto lg:py-10">
          <nav aria-label="GLIMPS docs" className="font-mono text-sm">
            <NavGroups pathname={pathname} variant="sidebar" />
          </nav>
        </aside>

        {/* Main article */}
        <main
          ref={mainRef}
          className="no-scrollbar smooth-scroll min-w-0 lg:h-full lg:overflow-y-auto py-8 lg:py-14"
        >
          <div className="flex items-center gap-2 font-mono text-xs text-muted-foreground mb-6">
            <Link to="/" className="hover:text-foreground transition-colors">
              GLIMPS Docs
            </Link>
            <span>›</span>
            <span className="text-foreground">{section}</span>
          </div>
          <h1 className="font-mono text-3xl sm:text-4xl font-semibold tracking-tight">
            {title}
          </h1>
          {intro && (
            <div className="mt-4 text-muted-foreground text-base leading-relaxed max-w-2xl">
              {intro}
            </div>
          )}
          <div
            className="mt-8 border-t pt-8 space-y-10"
            style={{ borderColor: "var(--color-border)" }}
          >
            {children}
          </div>
        </main>

        {/* Right TOC */}
        <aside className="no-scrollbar hidden lg:block lg:h-full lg:overflow-y-auto py-14">
          <div className="text-[10px] uppercase tracking-widest text-muted-foreground mb-3 font-mono">
            On this page
          </div>
          <ul
            className="border-l space-y-1 font-mono text-sm"
            style={{ borderColor: "var(--color-border)" }}
          >
            {toc.map((t) => {
              const isActive = t.id === active;
              return (
                <li key={t.id}>
                  <a
                    href={`#${t.id}`}
                    onClick={(e) => scrollToHeading(e, t.id)}
                    className={
                      "block -ml-px pl-4 py-1 border-l transition-colors " +
                      (isActive
                        ? "border-[var(--color-bar)] text-foreground"
                        : "border-transparent text-muted-foreground hover:text-foreground") +
                      (t.depth === 2 ? " pl-7 text-xs" : "")
                    }
                  >
                    {t.label}
                  </a>
                </li>
              );
            })}
          </ul>
        </aside>
        </div>
      </div>

      {/* Footer — full-width bar so it spans a wide section, not just the
          narrow article column. Sits below the scroll row on desktop. */}
      <footer
        className="border-t lg:shrink-0"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-6 flex flex-wrap gap-4 items-center justify-between font-mono text-xs text-muted-foreground">
          <span>
            <GlimpsMark size={15} className="inline-block align-[-0.24em] mr-1.5" /> glimps — a
            terminal you already have, just legible.
          </span>
          <span>MIT · docs v0.1</span>
        </div>
      </footer>
    </div>
  );
}

/* --------------- Content primitives shared by docs pages ------------- */

export function H2({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h2
      id={id}
      className="scroll-mt-20 font-mono text-2xl font-semibold tracking-tight pt-2"
    >
      {children}
    </h2>
  );
}

export function H3({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h3
      id={id}
      className="scroll-mt-20 font-mono text-lg font-semibold tracking-tight mt-6"
    >
      {children}
    </h3>
  );
}

export function P({ children }: { children: ReactNode }) {
  return <p className="text-[15px] leading-7 text-foreground/90">{children}</p>;
}

export function UL({ children }: { children: ReactNode }) {
  return (
    <ul className="list-disc pl-6 space-y-2 text-[15px] leading-7 text-foreground/90 marker:text-[var(--color-syn-dim)]">
      {children}
    </ul>
  );
}

export function Code({ children }: { children: ReactNode }) {
  return (
    <code className="px-1.5 py-0.5 rounded bg-muted text-foreground text-[13px] font-mono">
      {children}
    </code>
  );
}

type ShellLine =
  | string
  | { cmd: string }
  | { out: string; tone?: "muted" | "error" | "warn" | "success" };

export function Shell({ lines }: { lines: ShellLine[] }) {
  return (
    <div
      className="rounded-lg border overflow-hidden bg-[var(--color-terminal-bg)] my-2"
      style={{ borderColor: "var(--color-terminal-border)" }}
    >
      <div
        className="flex items-center gap-2 px-4 py-2 border-b"
        style={{
          background: "var(--color-terminal-chrome)",
          borderColor: "var(--color-terminal-border)",
        }}
      >
        <span className="h-2.5 w-2.5 rounded-full bg-[oklch(0.72_0.17_27)]" />
        <span className="h-2.5 w-2.5 rounded-full bg-[oklch(0.82_0.16_85)]" />
        <span className="h-2.5 w-2.5 rounded-full bg-[oklch(0.72_0.15_145)]" />
        <span className="ml-2 font-mono text-xs text-muted-foreground">shell</span>
      </div>
      <pre className="px-4 py-3 font-mono text-[13px] leading-6 overflow-x-auto">
        {lines.map((l, i) => {
          if (typeof l === "string") {
            return (
              <div key={i}>
                <span className="text-[var(--color-syn-dim)]">$ </span>
                <span>{l}</span>
              </div>
            );
          }
          if ("cmd" in l) {
            return (
              <div key={i}>
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌ </span>
                <span>{l.cmd}</span>
              </div>
            );
          }
          const tone = {
            muted: "text-[var(--color-syn-dim)]",
            error: "text-[var(--color-syn-error)]",
            warn: "text-[var(--color-syn-number)]",
            success: "text-[var(--color-syn-string)]",
          }[l.tone ?? "muted"];
          return (
            <div key={i} className={tone}>
              {l.out}
            </div>
          );
        })}
      </pre>
    </div>
  );
}

export function Callout({
  kind = "note",
  title,
  children,
}: {
  kind?: "note" | "warn";
  title: string;
  children: ReactNode;
}) {
  const color =
    kind === "warn" ? "var(--color-syn-error)" : "var(--color-syn-key)";
  return (
    <div
      className="rounded-lg border-l-4 border bg-muted/40 px-4 py-3 my-2"
      style={{ borderLeftColor: color, borderColor: "var(--color-border)" }}
    >
      <div className="font-mono text-xs uppercase tracking-widest mb-1" style={{ color }}>
        {title}
      </div>
      <div className="text-[14px] leading-6 text-foreground/90">{children}</div>
    </div>
  );
}

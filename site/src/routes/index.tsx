import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState, type ReactNode } from "react";
import { HeroVideoDialog } from "@/components/ui/hero-video-dialog";
import { GitHubStars } from "@/components/ui/github-stars";
import { Glimps } from "@/components/ui/glimps";
import { GlimpsMark } from "@/components/ui/glimps-mark";
import { canonical } from "@/lib/seo";
import { useTheme } from "@/hooks/use-theme";

/* ------------------------------------------------------------------ */
/*  DEMO VIDEO — replace these two with your real assets.              */
/*                                                                    */
/*  DEMO_VIDEO_SRC: an EMBED url (not a watch url). Examples:          */
/*    YouTube → https://www.youtube.com/embed/<VIDEO_ID>              */
/*    Vimeo   → https://player.vimeo.com/video/<VIDEO_ID>            */
/*    Self-hosted mp4 also works as the src.                          */
/*                                                                    */
/*  DEMO_POSTER: a 16:9 image in /public shown before play. Swap      */
/*    /demo-poster.svg for a real screenshot (e.g. /demo-poster.png). */
/* ------------------------------------------------------------------ */
const DEMO_VIDEO_SRC = "https://www.youtube.com/embed/7_5hzxcg3e0"; // GLIMPS explainer (youtu.be/7_5hzxcg3e0)
const DEMO_POSTER = "/demo-poster.svg";

/* The hero reel: the two recorded sessions from demo/*.tape, joined into one
   file so formatting plays to the end and failure intelligence follows, then
   the whole thing loops. A GIF cannot do that — the browser exposes no way to
   know when one has finished — which is why this is video rather than two
   <img> tags. The tapes in demo/ stay the source of truth: re-render them with
   scripts/render-demo.sh, then rebuild this with the ffmpeg command in
   demo/README.md. */
const DEMO_REEL = {
  src: "/demo.mp4",
  poster: "/demo-poster.jpg",
  title: "~/acme-api — zsh",
  width: 1200,
  height: 800,
  label: "a real session, unedited",
  description:
    "First: the command header marking where output begins, then JSON, log severity and an everyday ls formatted in place. Then: a mistyped command explained as exit 127, a pipeline failure the shell hid behind exit 0, Ctrl-C treated as a notice rather than an error, and a failing assertion pinned back into view.",
} as const;

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "GLIMPS — readable terminal output, clear command failures" },
      {
        name: "description",
        content:
          "Zero-config terminal formatter that structures recognized output and tells you how every command ended — with exit status, duration, and the error that mattered.",
      },
      { property: "og:title", content: "GLIMPS — readable output, clear failures" },
      {
        property: "og:description",
        content:
          "A zero-config PTY-based formatter that makes terminal output legible, surfaces failed commands, and gets out of the way when it isn't sure.",
      },
      { property: "og:url", content: canonical("/") },
    ],
    links: [{ rel: "canonical", href: canonical("/") }],
  }),
  component: Landing,
});

/* ------------------------------------------------------------------ */
/*  Primitives                                                         */
/* ------------------------------------------------------------------ */

function TerminalFrame({
  title = "~ / glimps",
  children,
  className = "",
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={
        "rounded-lg border shadow-[0_1px_0_rgba(0,0,0,0.02),0_20px_50px_-20px_rgba(0,0,0,0.15)] overflow-hidden bg-[var(--color-terminal-bg)] " +
        className
      }
      style={{ borderColor: "var(--color-terminal-border)" }}
    >
      <div
        className="flex items-center gap-2 px-4 py-2.5 border-b"
        style={{
          background: "var(--color-terminal-chrome)",
          borderColor: "var(--color-terminal-border)",
        }}
      >
        <span className="h-3 w-3 rounded-full bg-[oklch(0.72_0.17_27)]" />
        <span className="h-3 w-3 rounded-full bg-[oklch(0.82_0.16_85)]" />
        <span className="h-3 w-3 rounded-full bg-[oklch(0.72_0.15_145)]" />
        <span className="ml-3 font-mono text-xs text-muted-foreground truncate">
          {title}
        </span>
      </div>
      <div className="font-mono text-[13px] leading-6 overflow-x-auto">
        {children}
      </div>
    </div>
  );
}

function CmdHeader({ cmd, badge, time }: { cmd: string; badge?: string; time?: string }) {
  return (
    <div className="flex items-center gap-3 px-4 py-2 border-b border-dashed"
      style={{ borderColor: "var(--color-terminal-border)" }}>
      <span className="text-[var(--color-bar)] text-lg leading-none select-none">▌</span>
      <code className="text-foreground truncate">
        {cmd}
      </code>
      {badge && (
        <span
          className="ml-auto shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold tracking-wide uppercase"
          style={{
            background: "var(--color-muted)",
            color: "var(--color-muted-foreground)",
          }}
        >
          {badge}
        </span>
      )}
      {time && (
        <span className="ml-2 shrink-0 text-[10px] text-[var(--color-syn-dim)]">
          {time}
        </span>
      )}
    </div>
  );
}

function usePrefersReducedMotion() {
  const [reduce, setReduce] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduce(mq.matches);
    const onChange = (e: MediaQueryListEvent) => setReduce(e.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return reduce;
}

/* The hero reel. Autoplays muted and loops for everyone except a visitor who
   asked for reduced motion — they get the poster frame and real controls, so
   the demo is still reachable but nothing moves until they choose it. That is
   the one thing the GIFs this replaced could not offer: a GIF animates
   unconditionally, with no way to pause it. */
function HeroReel() {
  const reduce = usePrefersReducedMotion();
  return (
    <figure className="m-0">
      <figcaption className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
        <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span>{" "}
        {DEMO_REEL.label}
      </figcaption>
      <TerminalFrame title={DEMO_REEL.title}>
        <video
          src={DEMO_REEL.src}
          poster={DEMO_REEL.poster}
          width={DEMO_REEL.width}
          height={DEMO_REEL.height}
          autoPlay={!reduce}
          loop={!reduce}
          controls={reduce}
          muted
          playsInline
          preload="metadata"
          aria-label={DEMO_REEL.description}
          className="block w-full h-auto"
        />
      </TerminalFrame>
      <p className="mt-3 text-sm text-muted-foreground leading-relaxed">
        Formatting first, then what happens when a command fails.
      </p>
    </figure>
  );
}

/* ------------------------------------------------------------------ */
/*  Illustrations used by the explainer sections below.                 */
/* ------------------------------------------------------------------ */

function JsonTree() {
  const K = ({ c }: { c: string }) => <span className="text-[var(--color-syn-key)]">{c}</span>;
  const S = ({ c }: { c: string }) => <span className="text-[var(--color-syn-string)]">{c}</span>;
  const N = ({ c }: { c: string | number }) => (
    <span className="text-[var(--color-syn-number)]">{c}</span>
  );
  const B = ({ c }: { c: string }) => <span className="text-[var(--color-syn-keyword)]">{c}</span>;
  const P = ({ c }: { c: string }) => <span className="text-[var(--color-syn-dim)]">{c}</span>;
  return (
    <code>
      <P c="{" />{"\n"}
      {"  "}<K c='"user"' /><P c=": {" />{"\n"}
      {"    "}<K c='"id"' /><P c=": " /><N c="8421" /><P c="," />{"\n"}
      {"    "}<K c='"name"' /><P c=": " /><S c='"Ada Lovelace"' /><P c="," />{"\n"}
      {"    "}<K c='"email"' /><P c=": " /><S c='"ada@analytical.dev"' /><P c="," />{"\n"}
      {"    "}<K c='"active"' /><P c=": " /><B c="true" /><P c="," />{"\n"}
      {"    "}<K c='"role"' /><P c=": " /><S c='"admin"' /><P c="," />{"\n"}
      {"    "}<K c='"tags"' /><P c=": [" /><S c='"founder"' /><P c=", " /><S c='"math"' />
      <P c=", " /><S c='"poetry"' /><P c="]" />{"\n"}
      {"  "}<P c="}," />{"\n"}
      {"  "}<K c='"meta"' /><P c=": {" />{"\n"}
      {"    "}<K c='"count"' /><P c=": " /><N c="1" /><P c="," />{"\n"}
      {"    "}<K c='"latency_ms"' /><P c=": " /><N c="38" /><P c="," />{"\n"}
      {"    "}<K c='"cached"' /><P c=": " /><B c="false" />{"\n"}
      {"  "}<P c="}" />{"\n"}
      <P c="}" />
    </code>
  );
}

/* ------------------------------------------------------------------ */
/*  Nav                                                                */
/* ------------------------------------------------------------------ */

function Nav({ theme, onToggle }: { theme: "light" | "dark"; onToggle: () => void }) {
  return (
    <header className="relative z-10 border-b" style={{ borderColor: "var(--color-border)" }}>
      <div className="mx-auto max-w-7xl px-4 sm:px-6 py-3 sm:py-4 grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 sm:gap-4">
        <a href="#top" className="flex min-w-0 items-center gap-2 font-mono font-semibold">
          <GlimpsMark size={22} className="shrink-0" />
          <span className="truncate">glimps</span>
          <span className="ml-1 sm:ml-2 shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
            style={{ background: "var(--color-muted)", color: "var(--color-muted-foreground)" }}>
            beta
          </span>
        </a>
        <nav className="flex items-center gap-1 sm:gap-2 text-sm font-mono">
          <Link to="/about" className="hidden sm:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">about</Link>
          <Link to="/features" className="hidden sm:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">features</Link>
          <Link to="/commands" className="hidden md:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">commands</Link>
          <Link to="/installation" className="px-2.5 sm:px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">install</Link>
          <Link to="/feedback" className="hidden sm:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">feedback</Link>
          <GitHubStars className="hidden md:flex" />
          <button
            onClick={onToggle}
            aria-label="Toggle theme"
            className="ml-1 px-2.5 py-1.5 rounded border text-xs font-mono hover:bg-muted transition-colors"
            style={{ borderColor: "var(--color-border)" }}
          >
            {theme === "dark" ? "◐" : "◑"}
            <span className="hidden sm:inline ml-1">{theme === "dark" ? "light" : "dark"}</span>
          </button>
        </nav>
      </div>
    </header>

  );
}

/* ------------------------------------------------------------------ */
/*  Format gallery cards                                               */
/* ------------------------------------------------------------------ */

function LogsCard() {
  const Row = ({ level, color, msg, time }: { level: string; color: string; msg: string; time: string }) => (
    <div className="grid grid-cols-[auto_auto_1fr] gap-3 px-4 py-1">
      <span className="text-[var(--color-syn-dim)]">{time}</span>
      <span className="font-semibold" style={{ color }}>{level}</span>
      <span className="text-foreground truncate">{msg}</span>
    </div>
  );
  return (
    <TerminalFrame title="tail -f app.log">
      <CmdHeader cmd="tail -f app.log" badge="logs" time="14:22:07" />
      <div className="py-2">
        <Row time="14:22:01" level="INFO " color="var(--color-syn-string)" msg="server started on :8080" />
        <Row time="14:22:03" level="INFO " color="var(--color-syn-string)" msg="GET /api/user  200 12ms" />
        <Row time="14:22:04" level="WARN " color="var(--color-syn-number)" msg="cache miss for key=session:8421" />
        <Row time="14:22:05" level="INFO " color="var(--color-syn-string)" msg="POST /api/token  201 41ms" />
        <Row time="14:22:06" level="ERROR" color="var(--color-syn-error)" msg="upstream timeout after 3000ms" />
        <Row time="14:22:07" level="INFO " color="var(--color-syn-string)" msg="retrying request (1/3)" />
      </div>
    </TerminalFrame>
  );
}

function HttpCard() {
  return (
    <TerminalFrame title="curl -i api.example.com/orders/42">
      <CmdHeader cmd="curl -i api.example.com/orders/42" badge="http" />
      <div className="px-4 py-3 space-y-3">
        <div>
          <div className="text-[10px] uppercase tracking-wide text-[var(--color-syn-dim)] mb-1">status</div>
          <div>
            <span className="text-[var(--color-syn-dim)]">HTTP/2 </span>
            <span className="text-[var(--color-syn-string)] font-semibold">200</span>
            <span className="text-[var(--color-syn-dim)]"> OK</span>
          </div>
        </div>
        <div>
          <div className="text-[10px] uppercase tracking-wide text-[var(--color-syn-dim)] mb-1">headers</div>
          <div><span className="text-[var(--color-syn-key)]">content-type</span><span className="text-[var(--color-syn-dim)]">: </span><span className="text-[var(--color-syn-string)]">application/json</span></div>
          <div><span className="text-[var(--color-syn-key)]">cache-control</span><span className="text-[var(--color-syn-dim)]">: </span><span className="text-[var(--color-syn-string)]">no-store</span></div>
          <div><span className="text-[var(--color-syn-key)]">x-request-id</span><span className="text-[var(--color-syn-dim)]">: </span><span className="text-[var(--color-syn-string)]">a19f-882c</span></div>
        </div>
        <div>
          <div className="text-[10px] uppercase tracking-wide text-[var(--color-syn-dim)] mb-1">body</div>
          <div><span className="text-[var(--color-syn-dim)]">{"{"}</span></div>
          <div className="pl-4"><span className="text-[var(--color-syn-key)]">"order"</span><span className="text-[var(--color-syn-dim)]">: </span><span className="text-[var(--color-syn-number)]">42</span><span className="text-[var(--color-syn-dim)]">,</span></div>
          <div className="pl-4"><span className="text-[var(--color-syn-key)]">"paid"</span><span className="text-[var(--color-syn-dim)]">: </span><span className="text-[var(--color-syn-keyword)]">true</span></div>
          <div><span className="text-[var(--color-syn-dim)]">{"}"}</span></div>
        </div>
      </div>
    </TerminalFrame>
  );
}

function DiffCard() {
  const line = (sign: "+" | "-" | " ", text: string) => {
    const color =
      sign === "+" ? "var(--color-syn-string)" :
      sign === "-" ? "var(--color-syn-error)" :
      "var(--color-syn-dim)";
    return (
      <div className="px-4 py-0.5" style={{ color }}>
        <span className="inline-block w-4">{sign}</span>{text}
      </div>
    );
  };
  return (
    <TerminalFrame title="git diff HEAD~1 src/api.ts">
      <CmdHeader cmd="git diff HEAD~1 src/api.ts" badge="diff" />
      <div className="py-2">
        <div className="px-4 py-0.5 text-[var(--color-syn-dim)]">@@ -11,7 +11,8 @@ getUser</div>
        {line(" ", "export async function getUser(id: number) {")}
        {line("-", "  const r = await fetch(`/api/user/${id}`)")}
        {line("+", "  const r = await fetch(`/api/user/${id}`, { cache: 'no-store' })")}
        {line("-", "  return r.json()")}
        {line("+", "  if (!r.ok) throw new HttpError(r.status)")}
        {line("+", "  return r.json() as Promise<User>")}
        {line(" ", "}")}
      </div>
    </TerminalFrame>
  );
}

function StackCard() {
  return (
    <TerminalFrame title="python app.py">
      <CmdHeader cmd="python app.py" badge="trace" />
      <div className="px-4 py-3 space-y-1">
        <div className="text-[var(--color-syn-error)] font-semibold">Traceback (most recent call last):</div>
        <div className="text-[var(--color-syn-dim)]">  File "app/api/user.py", line 47, in resolve_user</div>
        <div className="text-[var(--color-syn-dim)]">  File "app/server.py", line 112, in handle_request</div>
        <div className="text-[var(--color-syn-error)] font-semibold">KeyError: 'id'</div>
      </div>
    </TerminalFrame>
  );
}

function TableCard() {
  const rows = [
    ["8421", "Ada Lovelace", "admin", "2024-11-04"],
    ["8422", "Grace Hopper", "admin", "2024-11-06"],
    ["8423", "Alan Turing", "member", "2024-11-09"],
    ["8424", "Katherine Johnson", "member", "2024-11-11"],
  ];
  return (
    <TerminalFrame title="psql -c 'select * from users limit 4'">
      <CmdHeader cmd="psql -c 'select * from users limit 4'" badge="table" />
      <div className="px-4 py-3">
        <div className="grid grid-cols-[80px_1fr_100px_120px] gap-4 pb-1 border-b" style={{ borderColor: "var(--color-terminal-border)" }}>
          {["id", "name", "role", "joined"].map((h) => (
            <span key={h} className="text-[var(--color-syn-key)] text-[11px] uppercase tracking-wide">{h}</span>
          ))}
        </div>
        {rows.map((r) => (
          <div key={r[0]} className="grid grid-cols-[80px_1fr_100px_120px] gap-4 py-1">
            <span className="text-[var(--color-syn-number)]">{r[0]}</span>
            <span>{r[1]}</span>
            <span className="text-[var(--color-syn-keyword)]">{r[2]}</span>
            <span className="text-[var(--color-syn-string)]">{r[3]}</span>
          </div>
        ))}
      </div>
    </TerminalFrame>
  );
}

function JsonMiniCard() {
  return (
    <TerminalFrame title="cat config.json">
      <CmdHeader cmd="cat config.json" badge="json" />
      <div className="px-4 py-3">
        <JsonTree />
      </div>
    </TerminalFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Copy-able command block                                            */
/* ------------------------------------------------------------------ */

function InstallBlock({ label, cmd }: { label: string; cmd: string }) {
  const [copied, setCopied] = useState(false);
  const lines = cmd.split("\n");
  return (
    <div>
      <div className="text-[11px] uppercase tracking-wide text-muted-foreground mb-2 font-mono">
        {label}
      </div>
      <div
        className="group relative rounded-lg border font-mono text-[13px] leading-6 bg-[var(--color-terminal-bg)]"
        style={{ borderColor: "var(--color-terminal-border)" }}
      >
        <div className="flex items-start gap-3 px-4 py-3 pr-14">
          <span className="text-[var(--color-bar)] leading-6 select-none">▌</span>
          <code className="flex-1 min-w-0 whitespace-pre-wrap break-all pb-1">
            {lines.map((line, i) => (
              <span key={i} className="block">
                <span className="text-[var(--color-syn-dim)]">$ </span>
                {line}
              </span>
            ))}
          </code>
        </div>
        <button
          onClick={() => {
            navigator.clipboard.writeText(cmd);
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          }}
          className="absolute top-2 right-2 px-2 py-1 rounded text-[11px] font-mono border hover:bg-muted transition-colors"
          style={{ borderColor: "var(--color-terminal-border)" }}
        >
          {copied ? "copied" : "copy"}
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                               */
/* ------------------------------------------------------------------ */

function Landing() {
  const [theme, toggleTheme] = useTheme();

  return (
    <div id="top" className="min-h-screen relative overflow-x-hidden">
      <Nav theme={theme} onToggle={toggleTheme} />

      {/* HERO */}
      <section className="relative z-[1] mx-auto max-w-7xl px-4 sm:px-6 pt-10 sm:pt-16 md:pt-24 pb-14 sm:pb-20">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-10 sm:gap-12 items-center">
          <div>
            <div className="inline-flex items-center gap-2 font-mono text-xs text-muted-foreground mb-5 sm:mb-6">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span>
              <span>zero-config · pass-through · MIT</span>
            </div>
            <h1 className="font-mono text-[clamp(1.5rem,7.4vw,2.25rem)] sm:text-4xl md:text-5xl lg:text-[clamp(2.4rem,3.7vw,3rem)] leading-[1.12] tracking-tight font-semibold">
              <span className="whitespace-nowrap">Your terminal output,</span>
              <br />
              <span className="whitespace-nowrap">
                <span className="text-[var(--color-syn-key)]">finally</span>{" "}
                <span className="text-[var(--color-syn-string)]">readable</span>
                <span className="text-[var(--color-bar)]">.</span>
              </span>
            </h1>
            <p className="mt-5 sm:mt-6 text-base md:text-lg text-muted-foreground max-w-xl leading-relaxed">
              Zero-config formatter that structures what it recognizes and tells you how
              every command ended — exit status, duration, and the error that mattered.
              It keeps your terminal; it just makes it legible.
            </p>
            <div className="mt-7 sm:mt-8 flex flex-wrap gap-3">
              <a
                href="#install"
                className="inline-flex items-center gap-2 rounded-md bg-primary text-primary-foreground px-5 py-2.5 font-mono text-sm font-medium hover:opacity-90 transition-opacity"
              >
                <span>Get started</span>
                <span className="text-[var(--color-syn-string)]">→</span>
              </a>
              <a
                href="#demo"
                className="inline-flex items-center gap-2 rounded-md border px-5 py-2.5 font-mono text-sm font-medium hover:bg-muted transition-colors"
                style={{ borderColor: "var(--color-border)" }}
              >
                See how it works
              </a>
            </div>
          </div>

          {/* A recorded session rather than a mockup: the hero's job is to prove
              the headline, and a real terminal does that better than an
              illustration of one. */}
          <HeroReel />
        </div>
      </section>

      {/* DEMO */}
      <section
        id="demo"
        className="relative z-[1] border-t"
        style={{ borderColor: "var(--color-border)" }}
      >
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="mx-auto max-w-2xl text-center mb-10 sm:mb-12">
            <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> see it in action
            </div>
            <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
              Watch <Glimps /> format a live session.
            </h2>
            <p className="mt-4 text-muted-foreground leading-relaxed">
              A short screen recording: real commands, real output — reformatted in place
              as it streams, with the <span className="font-mono text-foreground">▌</span>{" "}
              header marking where each command begins.
            </p>
          </div>
          <div className="mx-auto max-w-4xl">
            <HeroVideoDialog
              animationStyle="from-center"
              videoSrc={DEMO_VIDEO_SRC}
              thumbnailSrc={DEMO_POSTER}
              thumbnailAlt="GLIMPS reformatting terminal output — click to play the demo"
            />
          </div>

        </div>
      </section>

      {/* PROBLEM */}
      <section className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
            <div>
              <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> the problem
              </div>
              <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
                After a few commands, scrollback is a wall of text.
              </h2>
              <p className="mt-4 text-muted-foreground leading-relaxed">
                You can't tell where one command's output ended and the next began. JSON
                arrives as one long line. Logs blend together. You scroll, squint, and re-run
                the command just to find the answer you already had.
              </p>
            </div>
            <TerminalFrame title="~ / a normal afternoon">
              <pre className="px-4 py-4 text-[var(--color-syn-dim)] whitespace-pre-wrap break-all leading-6">
{`$ curl -s api.example.com/user
{"user":{"id":8421,"name":"Ada Lovelace","email":"ada@analytical.dev","active":true,"role":"admin","tags":["founder","math","poetry"]},"meta":{"count":1,"latency_ms":38,"cached":false}}
$ tail -n 3 app.log
2024-11-12T14:22:05.881Z ERROR upstream timeout after 3000ms trace_id=a19f-882c
2024-11-12T14:22:06.104Z INFO retrying request (1/3) trace_id=a19f-882c
2024-11-12T14:22:06.812Z INFO POST /api/token 201 41ms
$ git diff HEAD~1 src/api.ts
diff --git a/src/api.ts b/src/api.ts index 91a..c2b 100644 --- a/src/api.ts +++ b/src/api.ts @@ -12 +12 @@ -  const r = await fetch(\`/api/user/\${id}\`) +  const r = await fetch(\`/api/user/\${id}\`, { cache: 'no-store' })`}
              </pre>
            </TerminalFrame>
          </div>
        </div>
      </section>

      {/* TRANSFORM GALLERY */}
      <section id="transform" className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="max-w-2xl mb-12">
            <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> the transform
            </div>
            <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
              Same data. Now legible.
            </h2>
            <p className="mt-4 text-muted-foreground leading-relaxed">
              <Glimps /> recognizes what your commands emit and reformats it in place — with the
              command echoed above as a{" "}
              <span className="font-mono text-foreground">▌</span> header bar so you always
              know where output began.
            </p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <JsonMiniCard />
            <LogsCard />
            <HttpCard />
            <DiffCard />
            <StackCard />
            <TableCard />
          </div>
        </div>
      </section>

      {/* FAILURE INTELLIGENCE */}
      <section className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
            <div>
              <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span>{" "}
                failure intelligence
              </div>
              <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
                Know exactly how every command ended.
              </h2>
              <p className="mt-4 text-muted-foreground leading-relaxed">
                <Glimps /> attaches the result to the output that produced it: exit status,
                duration, and the most useful error line. Signals and negated commands are
                explained without turning every non-zero status into a red alarm.
              </p>
              <div className="mt-6 grid gap-3 text-sm">
                {[
                  "Pins the error that mattered, even when it scrolled away.",
                  "Decodes common exit codes and process signals.",
                  "Warns when an earlier pipeline stage failed behind exit 0.",
                  "Treats Ctrl-C and common SIGPIPE exits as neutral notices.",
                ].map((item) => (
                  <div key={item} className="flex gap-3">
                    <span className="font-mono text-[var(--color-syn-number)]">✓</span>
                    <span className="text-muted-foreground">{item}</span>
                  </div>
                ))}
              </div>
            </div>

            <TerminalFrame title="~/project — git push">
              <CmdHeader cmd="git push origin main" time="16:30:06" />
              <div className="px-4 py-3 font-mono text-[13px] leading-6 overflow-x-auto">
                <div className="text-[var(--color-syn-dim)]">
                  To https://github.com/example/project
                </div>
                <div>
                  <span className="text-[var(--color-syn-number)]"> ! [rejected] </span>
                  <span>main -&gt; main (fetch first)</span>
                </div>
                <div className="text-[var(--color-syn-error)]">
                  error: failed to push some refs
                </div>
                <div className="text-[var(--color-syn-dim)]">
                  hint: Updates were rejected because the remote contains work.
                </div>
                <div className="mt-2 text-[var(--color-syn-error)]">
                  ✗ failed exit 1 in 973ms
                </div>
                <div>
                  <span className="text-[var(--color-syn-error)]">↳ </span>
                  <span className="text-muted-foreground">error: failed to push some refs</span>
                  <span className="text-[var(--color-syn-dim)]"> (↑ 2 lines up)</span>
                </div>
                <div className="text-[var(--color-syn-error)]">
                  command failed: git push origin main
                </div>
              </div>
            </TerminalFrame>
          </div>
        </div>
      </section>

      {/* GETS OUT OF THE WAY */}
      <section className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
            <div>
              <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> it gets out of the way
              </div>
              <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
                When <Glimps /> isn't confident, it does nothing.
              </h2>
              <p className="mt-4 text-muted-foreground leading-relaxed">
                Full-screen apps, binary streams, and output that's already colored pass
                through untouched. No surprises. No mangled bytes. No rewriting things it
                doesn't fully understand.
              </p>
            </div>
            <div className="grid grid-cols-2 gap-3 font-mono text-sm">
              {[
                { name: "vim", note: "full-screen · TTY control" },
                { name: "ssh", note: "raw passthrough" },
                { name: "htop", note: "TUI · alternate screen" },
                { name: "less", note: "pager · owns the screen" },
                { name: "binary", note: "left untouched" },
                { name: "ansi-colored", note: "already styled · skipped" },
              ].map((x) => (
                <div
                  key={x.name}
                  className="rounded-md border px-4 py-3"
                  style={{ borderColor: "var(--color-border)" }}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-[var(--color-syn-dim)]">▌</span>
                    <span className="text-foreground">{x.name}</span>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">{x.note}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* TRUST */}
      <section id="trust" className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="max-w-2xl mb-10">
            <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> trust & safety
            </div>
            <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
              Four hard promises.
            </h2>
            <p className="mt-4 text-muted-foreground leading-relaxed">
              <Glimps /> sits in front of secrets, SSH sessions, and production output. It has
              to be honest about what it does and doesn't do.
            </p>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-px rounded-lg overflow-hidden border"
            style={{ borderColor: "var(--color-border)", background: "var(--color-border)" }}>
            {[
              {
                k: "01",
                t: "Nothing persistently logged or transmitted.",
                d: "No telemetry, cache, or output logs. A private temporary metadata channel is removed when the session ends.",
              },
              {
                k: "02",
                t: "Default to pass-through.",
                d: "GLIMPS only reformats output it's confident about. Everything else is byte-for-byte.",
              },
              {
                k: "03",
                t: "Your terminal is always restored.",
                d: "Even on a crash, GLIMPS resets terminal modes on exit. No dead sessions.",
              },
              {
                k: "04",
                t: "Instant off switch.",
                d: "Start a shell with GLIMPS=0 (or export it) and formatting is disabled completely — pure pass-through.",
              },
            ].map((p) => (
              <div key={p.k} className="p-6 bg-background">
                <div className="flex items-baseline gap-3 mb-2">
                  <span className="font-mono text-xs text-[var(--color-syn-dim)]">{p.k}</span>
                  <h3 className="font-mono font-semibold text-foreground">{p.t}</h3>
                </div>
                <p className="text-sm text-muted-foreground leading-relaxed pl-8">{p.d}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* INSTALL */}
      <section id="install" className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
            <div>
              <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> get started
              </div>
              <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
                One install. One guarded line.
              </h2>
              <p className="mt-4 text-muted-foreground leading-relaxed">
                No config file. No plugins. If <Glimps /> ever misbehaves, remove the line — or
                just{" "}
                <code className="px-1 py-0.5 rounded bg-muted text-foreground text-xs">
                  export GLIMPS=0
                </code>
                .
              </p>
              <p className="mt-4 text-sm text-muted-foreground">
                Prefer to try it first? Run{" "}
                <code className="px-1 py-0.5 rounded bg-muted text-foreground text-xs">
                  scripts/dogfood-macos.sh session
                </code>{" "}
                — it wraps a throwaway zsh and cleans up on exit, without touching your
                shell startup. Or just run{" "}
                <code className="px-1 py-0.5 rounded bg-muted text-foreground text-xs">
                  glimps
                </code>{" "}
                to start a wrapped shell and{" "}
                <code className="px-1 py-0.5 rounded bg-muted text-foreground text-xs">
                  exit
                </code>{" "}
                to leave.
              </p>
            </div>

            <div className="space-y-5">
              <InstallBlock
                label="1 · build & install (requires Rust)"
                cmd={"git clone https://github.com/Krishnarajan7/Glimps\ncd Glimps\ncargo install --path ."}
              />
              <InstallBlock
                label="2 · enable in your shell (near top of ~/.zshrc)"
                cmd='command -v glimps >/dev/null 2>&1 && eval "$(glimps init zsh)"'
              />
              <InstallBlock
                label="3 · or try without installing (macOS)"
                cmd="scripts/dogfood-macos.sh session"
              />
            </div>
          </div>
        </div>
      </section>

      {/* FOOTER */}
      <footer className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-7xl px-4 sm:px-6 py-8 sm:py-10 grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4">
          <div className="flex min-w-0 items-center gap-2 font-mono text-sm">
            <GlimpsMark size={18} className="shrink-0" />
            <span className="text-foreground">glimps</span>
            <span className="text-muted-foreground truncate">
              — a terminal you already have, just legible.
            </span>
          </div>
          <div className="flex items-center gap-4 text-sm font-mono text-muted-foreground">
            <a href="https://github.com/Krishnarajan7/Glimps" target="_blank" rel="noopener noreferrer" className="hover:text-foreground transition-colors">github</a>
            <Link to="/about" className="hover:text-foreground transition-colors">docs</Link>
            <Link to="/commands" className="hidden sm:inline hover:text-foreground transition-colors">commands</Link>
            <Link to="/feedback" className="hover:text-foreground transition-colors">feedback</Link>
            <span className="text-[var(--color-syn-dim)]">MIT</span>
          </div>
        </div>
      </footer>
    </div>
  );
}

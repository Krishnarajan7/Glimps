import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState, type ReactNode } from "react";

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      { title: "GLIMPS — your terminal output, finally readable" },
      {
        name: "description",
        content:
          "Zero-config terminal formatter that marks where your output starts and colors what it recognizes — JSON, logs, HTTP, diffs, and more. It keeps your terminal; it just makes it legible.",
      },
      { property: "og:title", content: "GLIMPS — your terminal output, finally readable" },
      {
        property: "og:description",
        content:
          "A zero-config PTY-based formatter that makes everyday terminal output legible, and gets out of the way when it isn't sure.",
      },
    ],
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
        <span className="text-[var(--color-syn-dim)]">$ </span>
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

/* ------------------------------------------------------------------ */
/*  Hero animation — raw JSON morphs into a formatted tree             */
/* ------------------------------------------------------------------ */

function HeroTerminal() {
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 4200);
    return () => clearInterval(id);
  }, []);

  const raw = `{"user":{"id":8421,"name":"Ada Lovelace","email":"ada@analytical.dev","active":true,"role":"admin","tags":["founder","math","poetry"]},"meta":{"count":1,"latency_ms":38,"cached":false}}`;

  return (
    <TerminalFrame title="~/projects/glimps — zsh">
      <div key={tick} className="relative">
        {/* Typed command */}
        <div className="px-4 pt-4 pb-2">
          <code>
            <span className="text-[var(--color-syn-dim)]">ada@paper </span>
            <span className="text-[var(--color-syn-string)]">~/projects/glimps</span>
            <span className="text-[var(--color-syn-dim)]"> $ </span>
            <span
              className="inline-block whitespace-nowrap overflow-hidden align-bottom term-cursor"
              style={{ animation: "type-in 1.1s steps(30) 0.15s both" }}
            >
              curl -s api.example.com/user
            </span>
          </code>
        </div>

        {/* Bar sweeps in */}
        <div className="relative pl-4">
          <div
            className="absolute left-4 top-0 bottom-0 w-[3px] origin-top"
            style={{
              background: "var(--color-bar)",
              animation: "bar-sweep 4.2s ease-out infinite",
            }}
          />

          {/* Raw output (fades out) */}
          <pre
            className="px-4 pt-2 pb-4 text-[var(--color-syn-dim)] whitespace-pre-wrap break-all"
            style={{ animation: "fade-swap-out 4.2s ease-in-out infinite" }}
          >
            {raw}
          </pre>

          {/* Formatted output (fades in over the raw) */}
          <pre
            className="absolute inset-0 px-4 pt-2 pb-4"
            style={{ animation: "fade-swap-in 4.2s ease-in-out infinite" }}
          >
            <JsonTree />
          </pre>
        </div>

        {/* Header bar label appears after sweep */}
        <div
          className="absolute left-4 top-[3.05rem] -translate-x-[calc(100%+8px)] hidden md:block"
          style={{ animation: "fade-swap-in 4.2s ease-in-out infinite" }}
        >
          <span className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
            style={{ background: "var(--color-muted)", color: "var(--color-muted-foreground)" }}>
            json
          </span>
        </div>
      </div>
    </TerminalFrame>
  );
}

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
      <div className="mx-auto max-w-6xl px-4 sm:px-6 py-3 sm:py-4 grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 sm:gap-4">
        <a href="#top" className="flex min-w-0 items-center gap-2 font-mono font-semibold">
          <span className="text-[var(--color-bar)] text-xl leading-none" aria-hidden="true">▌</span>
          <span className="truncate">glimps</span>
          <span className="ml-1 sm:ml-2 shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
            style={{ background: "var(--color-muted)", color: "var(--color-muted-foreground)" }}>
            beta
          </span>
        </a>
        <nav className="flex items-center gap-1 sm:gap-2 text-sm font-mono">
          <Link to="/about" className="hidden sm:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">about</Link>
          <Link to="/features" className="hidden sm:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">features</Link>
          <Link to="/installation" className="px-2.5 sm:px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">install</Link>
          <a href="https://github.com/Krishnarajan7/Glimps" target="_blank" rel="noopener noreferrer" className="hidden md:inline px-3 py-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors">github</a>
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
    const bg =
      sign === "+" ? "color-mix(in oklab, var(--color-syn-string) 10%, transparent)" :
      sign === "-" ? "color-mix(in oklab, var(--color-syn-error) 10%, transparent)" :
      "transparent";
    return (
      <div className="px-4 py-0.5" style={{ background: bg, color }}>
        <span className="inline-block w-4">{sign}</span>{text}
      </div>
    );
  };
  return (
    <TerminalFrame title="git diff HEAD~1 src/api.ts">
      <CmdHeader cmd="git diff HEAD~1 src/api.ts" badge="diff" />
      <div className="py-2">
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
        <div className="text-[var(--color-syn-dim)]">Traceback (most recent call last):</div>
        <div className="text-[var(--color-syn-dim)]">  File "<span className="text-[var(--color-syn-key)]">app/api/user.py</span>", line <span className="text-[var(--color-syn-number)]">47</span>, in <span className="text-foreground">resolve_user</span></div>
        <div className="text-[var(--color-syn-dim)]">  File "<span className="text-[var(--color-syn-key)]">app/server.py</span>", line <span className="text-[var(--color-syn-number)]">112</span>, in <span className="text-foreground">handle_request</span></div>
        <div>
          <span className="text-[var(--color-syn-error)] font-semibold">KeyError</span>
          <span className="text-[var(--color-syn-dim)]">: </span>
          <span className="text-[var(--color-syn-string)]">'id'</span>
        </div>
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
          <code className="flex-1 min-w-0 overflow-x-auto whitespace-pre pb-1">
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
  const [theme, setTheme] = useState<"light" | "dark">("light");
  useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
  }, [theme]);

  return (
    <div id="top" className="min-h-screen relative overflow-x-hidden">
      <Nav theme={theme} onToggle={() => setTheme((t) => (t === "light" ? "dark" : "light"))} />

      {/* HERO */}
      <section className="relative z-[1] mx-auto max-w-6xl px-4 sm:px-6 pt-10 sm:pt-16 md:pt-24 pb-14 sm:pb-20">
        <div className="grid lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)] gap-10 sm:gap-12 items-center">
          <div>
            <div className="inline-flex items-center gap-2 font-mono text-xs text-muted-foreground mb-5 sm:mb-6">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span>
              <span>zero-config · pass-through · MIT</span>
            </div>
            <h1 className="font-mono text-[2rem] sm:text-4xl md:text-5xl lg:text-[3.4rem] leading-[1.1] tracking-tight font-semibold">
              Your terminal output,<br />
              <span className="text-[var(--color-syn-key)]">finally</span>{" "}
              <span className="text-[var(--color-syn-string)]">readable</span>
              <span className="text-[var(--color-bar)]">.</span>
            </h1>
            <p className="mt-5 sm:mt-6 text-base md:text-lg text-muted-foreground max-w-xl leading-relaxed">
              Zero-config formatter that marks where your output starts and colors what it
              recognizes — JSON, logs, HTTP, diffs, and more. It keeps your terminal;
              it just makes it legible.
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
                href="#transform"
                className="inline-flex items-center gap-2 rounded-md border px-5 py-2.5 font-mono text-sm font-medium hover:bg-muted transition-colors"
                style={{ borderColor: "var(--color-border)" }}
              >
                See how it works
              </a>
            </div>
          </div>

          <HeroTerminal />
        </div>
      </section>


      {/* PROBLEM */}
      <section className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-6xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
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
        <div className="mx-auto max-w-6xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="max-w-2xl mb-12">
            <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> the transform
            </div>
            <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
              Same bytes. Now legible.
            </h2>
            <p className="mt-4 text-muted-foreground leading-relaxed">
              GLIMPS recognizes what your commands emit and reformats it in place — with the
              command echoed above as a{" "}
              <span className="font-mono text-foreground">▌</span> header bar so you always
              know where output began.
            </p>
          </div>

          <div className="grid md:grid-cols-2 gap-6">
            <JsonMiniCard />
            <LogsCard />
            <HttpCard />
            <DiffCard />
            <StackCard />
            <TableCard />
          </div>
        </div>
      </section>

      {/* GETS OUT OF THE WAY */}
      <section className="relative z-[1] border-t" style={{ borderColor: "var(--color-border)" }}>
        <div className="mx-auto max-w-6xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
            <div>
              <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> it gets out of the way
              </div>
              <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
                When GLIMPS isn't confident, it does nothing.
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
        <div className="mx-auto max-w-6xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="max-w-2xl mb-10">
            <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
              <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> trust & safety
            </div>
            <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
              Four hard promises.
            </h2>
            <p className="mt-4 text-muted-foreground leading-relaxed">
              GLIMPS sits in front of secrets, SSH sessions, and production output. It has
              to be honest about what it does and doesn't do.
            </p>
          </div>

          <div className="grid md:grid-cols-2 gap-px rounded-lg overflow-hidden border"
            style={{ borderColor: "var(--color-border)", background: "var(--color-border)" }}>
            {[
              {
                k: "01",
                t: "Nothing logged, stored, or transmitted.",
                d: "No telemetry, ever. Output is formatted in memory and streamed back to your terminal.",
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
                d: "Set GLIMPS=0 and it disables completely for the current command or shell.",
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
        <div className="mx-auto max-w-6xl px-4 sm:px-6 py-14 sm:py-20">
          <div className="grid md:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-10 items-start">
            <div>
              <div className="font-mono text-xs uppercase tracking-widest text-muted-foreground mb-3">
                <span className="text-[var(--color-bar)]" aria-hidden="true">▌</span> get started
              </div>
              <h2 className="font-mono text-2xl md:text-3xl font-semibold leading-tight">
                One install. One guarded line.
              </h2>
              <p className="mt-4 text-muted-foreground leading-relaxed">
                No config file. No plugins. If GLIMPS ever misbehaves, remove the line — or
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
        <div className="mx-auto max-w-6xl px-4 sm:px-6 py-8 sm:py-10 grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4">
          <div className="flex min-w-0 items-center gap-2 font-mono text-sm">
            <span className="text-[var(--color-bar)] text-lg" aria-hidden="true">▌</span>
            <span className="text-foreground">glimps</span>
            <span className="text-muted-foreground truncate">
              — a terminal you already have, just legible.
            </span>
          </div>
          <div className="flex items-center gap-4 text-sm font-mono text-muted-foreground">
            <a href="https://github.com/Krishnarajan7/Glimps" target="_blank" rel="noopener noreferrer" className="hover:text-foreground transition-colors">github</a>
            <Link to="/about" className="hover:text-foreground transition-colors">docs</Link>
            <span className="text-[var(--color-syn-dim)]">MIT</span>
          </div>
        </div>
      </footer>
    </div>
  );
}

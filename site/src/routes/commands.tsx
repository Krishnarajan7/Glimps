import { Link, createFileRoute } from "@tanstack/react-router";
import {
  Callout,
  DocsLayout,
  H2,
  P,
  type TocItem,
} from "../components/DocsLayout";
import { Glimps } from "@/components/ui/glimps";
import { canonical } from "@/lib/seo";

export const Route = createFileRoute("/commands")({
  head: () => ({
    meta: [
      { title: "Command coverage — GLIMPS" },
      {
        name: "description",
        content:
          "Browse the commands, output shapes, and file formats GLIMPS recognizes, or request support for a missing command.",
      },
      { property: "og:title", content: "GLIMPS command coverage" },
      {
        property: "og:description",
        content:
          "A precise catalogue of command-aware, content-aware, and file-aware formatting in GLIMPS.",
      },
      { property: "og:url", content: canonical("/commands") },
    ],
    links: [{ rel: "canonical", href: canonical("/commands") }],
  }),
  component: CommandsPage,
});

const toc: TocItem[] = [
  { id: "how-it-works", label: "How coverage works" },
  { id: "files", label: "Files & navigation" },
  { id: "search", label: "Search & documentation" },
  { id: "development", label: "Development" },
  { id: "network", label: "Network & HTTP" },
  { id: "system", label: "System & storage" },
  { id: "data", label: "Data & file formats" },
  { id: "request", label: "Request a command" },
];

type CoverageItem = {
  commands: string[];
  detail: string;
};

function CoverageGrid({ items }: { items: CoverageItem[] }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      {items.map((item) => (
        <article
          key={item.commands.join(" ")}
          className="rounded-lg border bg-muted/20 p-4"
          style={{ borderColor: "var(--color-border)" }}
        >
          <div className="mb-2 flex flex-wrap gap-1.5">
            {item.commands.map((command) => (
              <code
                key={command}
                className="rounded bg-muted px-1.5 py-0.5 font-mono text-[12px] text-foreground"
              >
                {command}
              </code>
            ))}
          </div>
          <p className="text-[14px] leading-6 text-muted-foreground">
            {item.detail}
          </p>
        </article>
      ))}
    </div>
  );
}

function CommandsPage() {
  return (
    <DocsLayout
      section="Reference"
      title="Command coverage"
      intro={
        <>
          What <Glimps quiet /> recognizes today, where command context matters,
          and what safely passes through unchanged.
        </>
      }
      toc={toc}
    >
      <section className="space-y-4">
        <H2 id="how-it-works">How coverage works</H2>
        <P>
          <Glimps quiet /> does not maintain one giant list of commands and
          blindly color their output. It combines three signals: the command you
          ran, the shape of its output, and—when reading a file—the filename.
          This lets the same HTML, JSON, log, or source formatter work across
          compatible producers without teaching GLIMPS every possible pipeline.
        </P>
        <div className="grid gap-3 sm:grid-cols-3">
          {[
            [
              "content-aware",
              "Recognizes complete documents and structured lines from their output shape.",
            ],
            [
              "command-aware",
              "Uses command semantics for tables, status output, summaries, and diagnostics.",
            ],
            [
              "file-aware",
              "Uses reader commands plus the filename to select the appropriate syntax formatter.",
            ],
          ].map(([label, detail]) => (
            <div
              key={label}
              className="rounded-lg border p-4"
              style={{ borderColor: "var(--color-border)" }}
            >
              <div className="mb-2 font-mono text-xs font-semibold text-foreground">
                {label}
              </div>
              <p className="text-[13px] leading-5 text-muted-foreground">
                {detail}
              </p>
            </div>
          ))}
        </div>
        <Callout title="confidence before color">
          Entries below describe supported output shapes, not a promise that
          every flag of every command produces the same schema. If GLIMPS is not
          confident, it leaves the bytes alone.
        </Callout>
      </section>

      <section className="space-y-4">
        <H2 id="files">Files &amp; navigation</H2>
        <CoverageGrid
          items={[
            {
              commands: ["ls", "find", "du", "df"],
              detail:
                "Directory, size, and filesystem tables with semantic columns and threshold-aware capacity colors.",
            },
            {
              commands: ["pwd", "cd -"],
              detail:
                "Readable working-directory and previous-directory breadcrumbs without misleading success output.",
            },
            {
              commands: ["cat", "head", "tail", "sed", "more", "nl"],
              detail:
                "File-aware readers. nl retains its number gutter while the detected file content is formatted.",
            },
            {
              commands: ["touch", "mkdir", "rm", "killall"],
              detail:
                "Conservative breadcrumbs for otherwise silent successful actions. Failed actions never claim completion.",
            },
          ]}
        />
      </section>

      <section className="space-y-4">
        <H2 id="search">Search &amp; documentation</H2>
        <CoverageGrid
          items={[
            {
              commands: ["rg", "grep", "egrep", "fgrep"],
              detail:
                "Structured search results with filenames, line numbers, separators, and matching text kept distinct.",
            },
            {
              commands: ["history", "history | …"],
              detail:
                "History listings and common count pipelines, while preserving the shell's real output and ordering.",
            },
            {
              commands: ["whereis"],
              detail:
                "Command names and resolved executable or manual paths are separated for faster scanning.",
            },
            {
              commands: ["man", "whatis", "apropos", "--help"],
              detail:
                "Manual pages, index searches, usage blocks, headings, flags, and descriptions.",
            },
          ]}
        />
      </section>

      <section className="space-y-4">
        <H2 id="development">Development</H2>
        <CoverageGrid
          items={[
            {
              commands: [
                "git status",
                "git branch",
                "git log",
                "git show",
                "git diff",
              ],
              detail:
                "Git state, refs, commits, diff content, and stat/name-status variants with command-specific meaning.",
            },
            {
              commands: ["cargo build", "cargo test", "cargo check"],
              detail:
                "Build progress, compiler diagnostics, per-test verdicts, and summaries—including b, t, and c aliases.",
            },
            {
              commands: ["kubectl get pods"],
              detail:
                "Pod tables with status-aware fields. Other kubectl shapes currently remain untouched.",
            },
            {
              commands: ["diff output", "stack traces"],
              detail:
                "Content-aware unified diffs plus Rust, Python, and common exception or stack-trace lines.",
            },
          ]}
        />
      </section>

      <section className="space-y-4">
        <H2 id="network">Network &amp; HTTP</H2>
        <CoverageGrid
          items={[
            {
              commands: ["curl -I", "curl -i", "curl -O"],
              detail:
                "HTTP status and headers, JSON or HTML bodies when recognizable, and curl transfer progress.",
            },
            {
              commands: ["ping", "ping6"],
              detail:
                "Replies, latency, packet loss, and final round-trip summaries.",
            },
            {
              commands: ["dig", "nslookup", "host"],
              detail:
                "DNS sections, record types, names, addresses, TTLs, and query summaries.",
            },
            {
              commands: ["whois"],
              detail:
                "Registration fields, contacts, addresses, country codes, dates, URLs, and abuse contacts by meaning.",
            },
          ]}
        />
      </section>

      <section className="space-y-4">
        <H2 id="system">System &amp; storage</H2>
        <CoverageGrid
          items={[
            {
              commands: ["ps", "lsof -i"],
              detail:
                "Process and network-process tables with identity, resource, state, and command fields separated.",
            },
            {
              commands: ["GetFileInfo", "xattr -l", "diskutil info"],
              detail:
                "macOS file, attribute, disk, volume, capacity, protocol, and health metadata.",
            },
            {
              commands: ["ifconfig", "netstat -rn", "route get default"],
              detail:
                "Interface and route output, including addresses, flags, gateways, and link details.",
            },
            {
              commands: [
                "networksetup",
                "scutil --dns",
                "launchctl list",
                "pmset -g",
              ],
              detail:
                "Selected macOS network, DNS, service, and power-management views.",
            },
          ]}
        />
      </section>

      <section className="space-y-4">
        <H2 id="data">Data &amp; file formats</H2>
        <CoverageGrid
          items={[
            {
              commands: ["JSON", "JSON Lines", "HTML", "HTTP"],
              detail:
                "Whole-document or structured streaming detection when the complete shape validates.",
            },
            {
              commands: ["CSV", "TSV", "PSV", "semicolon-delimited"],
              detail:
                "Quoted-field-aware parsing and width-bounded tables instead of coloring raw separators.",
            },
            {
              commands: ["YAML", "TOML", "INI", ".env", ".gitignore", ".htaccess"],
              detail:
                "Project configuration, dotenv, ignore, .gitleaksignore, and semantic Apache directives through file-aware readers.",
            },
            {
              commands: ["psql", "sqlite3", "mysql", "mariadb", "duckdb"],
              detail:
                "Small database result tables with headers and aligned values; oversized tables pass through safely.",
            },
            {
              commands: [
                "Rust",
                "JavaScript",
                "TypeScript",
                "Python",
                "shell",
                "Go",
              ],
              detail:
                "Source-aware readers for these languages plus Java, Kotlin, Swift, Ruby, PHP, CSS, and C/C++ families.",
            },
            {
              commands: ["Markdown", "SQL", "Dockerfile", "Makefile"],
              detail:
                "File-aware structure and syntax, including embedded HTML in Markdown rather than plain-text fallback.",
            },
          ]}
        />
      </section>

      <section
        id="request"
        className="scroll-mt-20 space-y-4 rounded-xl border bg-muted/30 p-5 sm:p-6"
        style={{ borderColor: "var(--color-border)" }}
      >
        <H2 id="request-title">Missing a command?</H2>
        <P>
          Send us the command, redacted real output, and the fields that should
          stand out. That evidence helps us build a semantic formatter instead
          of adding decorative color that breaks on the next machine.
        </P>
        <div className="flex flex-wrap gap-3">
          <a
            href="https://github.com/Krishnarajan7/Glimps/issues/new?template=formatter_request.yml"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center rounded-md bg-foreground px-4 py-2 font-mono text-sm text-background transition-opacity hover:opacity-85"
          >
            Request a formatter ↗
          </a>
          <Link
            to="/features"
            className="inline-flex items-center rounded-md border px-4 py-2 font-mono text-sm text-foreground hover:bg-muted"
            style={{ borderColor: "var(--color-border)" }}
          >
            Read the guarantees
          </Link>
        </div>
        <p className="text-xs leading-5 text-muted-foreground">
          Never paste passwords, tokens, cookies, private keys, or unredacted
          customer data.
        </p>
      </section>
    </DocsLayout>
  );
}

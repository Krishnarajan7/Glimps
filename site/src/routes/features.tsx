import { createFileRoute } from "@tanstack/react-router";
import {
  DocsLayout,
  H2,
  H3,
  P,
  UL,
  Code,
  Shell,
  Callout,
  type TocItem,
} from "../components/DocsLayout";

export const Route = createFileRoute("/features")({
  head: () => ({
    meta: [
      { title: "Features — GLIMPS" },
      {
        name: "description",
        content:
          "Every format GLIMPS recognizes, the guarantees it offers, and the behavior you can rely on. JSON, logs, HTTP, diffs, stack traces, tables, and the pass-through rules.",
      },
      { property: "og:title", content: "GLIMPS Features" },
      {
        property: "og:description",
        content:
          "The full list of formats GLIMPS recognizes and the behavior you can rely on.",
      },
    ],
  }),
  component: FeaturesPage,
});

const toc: TocItem[] = [
  { id: "highlight", label: "Feature highlight" },
  { id: "formats", label: "Recognized formats" },
  { id: "json", label: "JSON", depth: 2 },
  { id: "logs", label: "Structured logs", depth: 2 },
  { id: "http", label: "HTTP exchanges", depth: 2 },
  { id: "diffs", label: "Diffs", depth: 2 },
  { id: "stack", label: "Stack traces", depth: 2 },
  { id: "tables", label: "Small tables", depth: 2 },
  { id: "command-aware", label: "Command-aware", depth: 2 },
  { id: "bar", label: "The ▌ command bar" },
  { id: "passthrough", label: "Pass-through rules" },
  { id: "kill-switch", label: "Kill switch" },
  { id: "performance", label: "Performance" },
  { id: "guarantees", label: "Guarantees" },
];

function FeaturesPage() {
  return (
    <DocsLayout
      section="Features"
      title="Features"
      intro={
        <>
          The features GLIMPS supports and the promises it keeps. Everything here is
          on by default — there is nothing to configure.
        </>
      }
      toc={toc}
    >
      <section className="space-y-4">
        <H2 id="highlight">Feature highlight</H2>
        <P>
          A short list of what people notice first. Each item is covered in more depth
          below.
        </P>
        <UL>
          <li>
            <b>Command bar.</b> A <Code>▌</Code> is drawn at the start of every command's
            output so scrollback has visible anchors.
          </li>
          <li>
            <b>Streaming JSON.</b> Compact JSON is reflowed and colored as it arrives —
            no need to pipe through <Code>jq</Code>.
          </li>
          <li>
            <b>Log levels.</b> Common log formats have their level highlighted (INFO,
            WARN, ERROR) without touching the message.
          </li>
          <li>
            <b>HTTP responses.</b> Status lines are colored by class (2xx / 4xx / 5xx),
            headers are dimmed, body is treated as JSON when appropriate.
          </li>
          <li>
            <b>Diffs & stack traces.</b> Line-level color for added/removed lines and
            for the exception header of a stack trace.
          </li>
        </UL>
      </section>

      <section className="space-y-6">
        <H2 id="formats">Recognized formats</H2>

        <div className="space-y-3">
          <H3 id="json">JSON</H3>
          <P>
            GLIMPS reformats a JSON document only when the entire output of a command
            parses as JSON. Partial or mixed output is left alone. Keys, strings,
            numbers, and literals ( <Code>true</Code>, <Code>false</Code>,{" "}
            <Code>null</Code>) get distinct colors.
          </P>
          <Shell
            lines={[
              { cmd: "curl -s api.example.com/user" },
              { out: '{ "user": { "id": 8421, "name": "Ada Lovelace", "active": true } }' },
            ]}
          />
        </div>

        <div className="space-y-3">
          <H3 id="logs">Structured logs</H3>
          <P>
            Timestamps and levels are recognized in the most common log shapes (RFC3339,
            <Code>ISO8601</Code>, systemd-style, and bracketed levels like{" "}
            <Code>[INFO]</Code>). The message body is not modified.
          </P>
          <Shell
            lines={[
              { cmd: "tail -f app.log" },
              { out: "14:22:04  WARN  cache miss for key=session:8421" },
              { out: "14:22:06  ERROR upstream timeout after 3000ms" },
            ]}
          />
        </div>

        <div className="space-y-3">
          <H3 id="http">HTTP exchanges</H3>
          <P>
            When GLIMPS sees an HTTP status line ( <Code>HTTP/1.1 200 OK</Code>,{" "}
            <Code>HTTP/2 404</Code>) followed by header-like lines, it colors the status
            by class and dims the header names.
          </P>
        </div>

        <div className="space-y-3">
          <H3 id="diffs">Diffs</H3>
          <P>
            Unified diffs (from <Code>git</Code>, <Code>diff -u</Code>, or patch files)
            get red/green line backgrounds with a small tint that stays readable on both
            themes. Hunk headers are dimmed but preserved verbatim.
          </P>
        </div>

        <div className="space-y-3">
          <H3 id="stack">Stack traces</H3>
          <P>
            The exception line is colored, file paths and line numbers are highlighted,
            and framework noise is dimmed. GLIMPS recognizes Rust panics and Python
            tracebacks.
          </P>
        </div>

        <div className="space-y-3">
          <H3 id="tables">Small tables</H3>
          <P>
            Fixed-width tables (from <Code>psql</Code>, <Code>kubectl</Code>,{" "}
            <Code>docker ps</Code>) get a subtle header underline and column-aligned
            values. Long tables are passed through untouched to avoid reflow surprises.
          </P>
        </div>

        <div className="space-y-3">
          <H3 id="command-aware">Command-aware output</H3>
          <P>
            Beyond whole-document and streaming detection, GLIMPS knows the shape of many
            everyday commands and files and formats them accordingly: Git (
            <Code>status</Code>, <Code>branch</Code>, <Code>log</Code>, <Code>stat</Code>),
            CSV/TSV, SQL, config files (YAML, TOML, INI, dotenv), JSON-lines (
            <Code>.jsonl</Code>), source-code files shown through reader commands (
            <Code>cat</Code>, <Code>head</Code>), directory and system tools (
            <Code>ls</Code>, <Code>find</Code>, <Code>du</Code>, <Code>df</Code>,{" "}
            <Code>ps</Code>, <Code>dig</Code>), <Code>man</Code>/help output, and Markdown
            files.
          </P>
          <P>
            Every formatter is on by default with zero config, and each can be toggled
            individually under <Code>[formatters]</Code> in <Code>~/.glimpsrc</Code>.
          </P>
        </div>
      </section>

      <section className="space-y-4">
        <H2 id="bar">The ▌ command bar</H2>
        <P>
          The most visible thing GLIMPS does. A short vertical bar is drawn just before
          the first line of every command's output. It gives scrollback a structural
          rhythm and lets you scroll by "command", not by "line".
        </P>
        <P>
          The bar is a real character rendered in your terminal's foreground color — it
          survives copy/paste as a normal <Code>U+258C</Code> (LEFT HALF BLOCK), and can
          be stripped with <Code>sed 's/▌//g'</Code> if you ever need to.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="passthrough">Pass-through rules</H2>
        <P>GLIMPS will never touch output when any of these are true:</P>
        <UL>
          <li>
            The program has taken over the alternate (full) screen (<Code>vim</Code>,{" "}
            <Code>less</Code>, <Code>htop</Code>, <Code>fzf</Code>).
          </li>
          <li>The stream contains ANSI color escapes already.</li>
          <li>The output is binary (non-UTF-8 bytes).</li>
          <li>The destination is not a TTY (output is piped or redirected).</li>
          <li>A no-echo password prompt is on screen.</li>
          <li>
            The command is an SSH session (<Code>ssh</Code>).
          </li>
        </UL>
        <Callout title="why">
          Reformatting bytes you didn't ask to change is worse than showing them raw.
          Pass-through is not a fallback — it's the default.
        </Callout>
      </section>

      <section className="space-y-4">
        <H2 id="kill-switch">Kill switch</H2>
        <P>
          GLIMPS wraps your whole shell, so the switch is per-shell or per-environment,
          not per-command. Three ways to turn it off, in order of scope:
        </P>
        <UL>
          <li>
            Raw shell: <Code>GLIMPS=0 zsh</Code> starts an unwrapped shell.
          </li>
          <li>
            Whole environment: <Code>export GLIMPS=0</Code> keeps future shells raw.
          </li>
          <li>
            Persistent: set <Code>enabled = false</Code> in <Code>~/.glimpsrc</Code>, or
            remove the <Code>eval "$(glimps init …)"</Code> line from your shell startup
            file.
          </li>
        </UL>
      </section>

      <section className="space-y-4">
        <H2 id="performance">Performance</H2>
        <P>
          GLIMPS processes output as it streams. Its recognition step is a single-pass
          scan, and buffering is hard-bounded: a whole-document formatter holds at most
          1 MiB (<Code>buffer_cap = 1048576</Code>) before falling back to pass-through,
          and streaming line formatters cap a single line at 64 KiB (
          <Code>line_cap = 65536</Code>). On a typical laptop the overhead is well under
          a millisecond per command.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="guarantees">Guarantees</H2>
        <UL>
          <li>No telemetry, ever. No network calls.</li>
          <li>
            Nothing is written to disk. GLIMPS only ever (optionally) reads your{" "}
            <Code>~/.glimpsrc</Code> config — no state directory, no cache, no logs.
          </li>
          <li>Your terminal is restored on exit, even after a crash.</li>
          <li>Copy/paste from your terminal yields exactly the bytes you see.</li>
        </UL>
      </section>
    </DocsLayout>
  );
}

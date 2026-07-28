import { createFileRoute } from "@tanstack/react-router";
import {
  DocsLayout,
  H2,
  H3,
  P,
  UL,
  Code,
  Callout,
  type TocItem,
} from "../components/DocsLayout";
import { canonical } from "@/lib/seo";
import { Glimps } from "@/components/ui/glimps";

export const Route = createFileRoute("/about")({
  head: () => ({
    meta: [
      { title: "About GLIMPS — the zero-config terminal formatter" },
      {
        name: "description",
        content:
          "Why GLIMPS exists, what it is, what it is not, and the design principles behind a formatter that stays out of your way.",
      },
      { property: "og:title", content: "About GLIMPS" },
      {
        property: "og:description",
        content:
          "A short honest read on GLIMPS: what it does, what it refuses to do, and why your terminal stays yours.",
      },
      { property: "og:url", content: canonical("/about") },
    ],
    links: [{ rel: "canonical", href: canonical("/about") }],
  }),
  component: AboutPage,
});

const toc: TocItem[] = [
  { id: "overview", label: "Overview" },
  { id: "why", label: "Why GLIMPS exists" },
  { id: "what-it-is", label: "What it is" },
  { id: "what-it-isnt", label: "What it is not" },
  { id: "principles", label: "Design principles" },
  { id: "history", label: "Project history" },
  { id: "license", label: "License & credits" },
];

function AboutPage() {
  return (
    <DocsLayout
      section="About"
      title="About GLIMPS"
      intro={
        <>
          <Glimps quiet /> is a small, zero-config formatter that sits between your shell and your
          screen. It marks where each command's output begins and colors what it can
          confidently recognize. Everything else passes through untouched.
        </>
      }
      toc={toc}
    >
      <section className="space-y-4">
        <H2 id="overview">Overview</H2>
        <P>
          <Glimps quiet /> is not a new terminal or a shell. It re-execs your shell (bash or
          zsh) inside a PTY it supervises, with lightweight shell integration providing
          trusted command boundaries and exit status. A single guarded init line turns it
          on. From there it formats recognized output and keeps each command's result beside
          the output that produced it.
        </P>
        <P>
          The design goal is intentionally narrow: make the text you already produce
          legible, and never do anything else. <Glimps quiet /> is zero-config by default — an optional{" "}
          <Code>~/.glimpsrc</Code> (TOML) exists if you want to tune it, but nothing is
          required. There is no plugin system, no theme registry, no server, and no daemon.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="why">Why <Glimps quiet /> exists</H2>
        <P>
          Terminals are still the fastest way to see what a program actually did. But the
          text a program emits was designed for machines, not eyes. After a busy hour of
          <Code>curl</Code>, <Code>tail</Code>, and <Code>git diff</Code>, your scrollback
          becomes a wall of characters with no visual anchors. You can't find where the
          last command's output began, and you can't tell an error line from an info line
          without reading every one of them.
        </P>
        <P>
          The usual fixes are heavy: switch terminals, adopt a TUI, or install a suite of
          language-specific pretty-printers. <Glimps quiet /> takes the opposite bet — that a very
          small amount of formatting, applied only when it's obviously safe, is enough to
          make everyday terminal work materially better.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="what-it-is">What it is</H2>
        <UL>
          <li>
            A single native binary that supervises the PTY your shell runs in, assisted by
            lightweight shell hooks that report command boundaries and status.
          </li>
          <li>
            A streaming formatter that recognizes a fixed set of well-known formats and
            reflows them inline.
          </li>
          <li>
            A visible marker — the <Code>▌</Code> bar — placed at the start of every
            command's output so you always know where output begins.
          </li>
          <li>
            Fully local. Nothing is transmitted or persistently logged; temporary metadata
            is private to the session and removed on exit.
          </li>
        </UL>
      </section>

      <section className="space-y-4">
        <H2 id="what-it-isnt">What it is not</H2>
        <UL>
          <li>Not a terminal emulator. You keep iTerm, Alacritty, Ghostty, whatever you already use.</li>
          <li>Not a shell. You keep bash or zsh.</li>
          <li>Not a REPL, not an AI assistant, not a scrollback search tool.</li>
          <li>Not a general "pretty printer" — <Glimps quiet /> refuses to reformat anything it isn't confident about.</li>
        </UL>
        <Callout title="on trust">
          If <Glimps quiet /> ever misbehaves, turn it off. Because it wraps the whole shell, the
          switch is per-shell or per-environment: start a raw shell with{" "}
          <Code>GLIMPS=0 zsh</Code>, keep future shells raw with <Code>export GLIMPS=0</Code>,
          or set <Code>enabled = false</Code> in <Code>~/.glimpsrc</Code>. Removing the init
          line works too. There is no daemon to kill and no state to clean up.
        </Callout>
      </section>

      <section className="space-y-4">
        <H2 id="principles">Design principles</H2>
        <H3 id="pass-through">Default to pass-through</H3>
        <P>
          When in doubt, <Glimps quiet /> emits bytes exactly as it received them. Binary streams,
          already-colored output, full-screen apps, and formats it doesn't recognize are
          all passed through verbatim. The tool would rather show plain text than pretend
          to understand something it doesn't.
        </P>
        <H3 id="honest">Be honest about what it did</H3>
        <P>
          The <Code>▌</Code> bar makes GLIMPS's presence visible on every command it
          touched. There is no invisible rewriting. If your terminal looks unusual, it's
          because <Glimps quiet /> did something — and you can see exactly where.
        </P>
        <H3 id="silent">Quiet about itself</H3>
        <P>
          <Glimps quiet /> adds useful command context — boundaries, status, duration, and
          failure summaries — but no startup banner, update prompt, or promotional noise.
          A short farewell appears when the session exits. Usage and version information stay
          explicit through <Code>glimps --help</Code> and <Code>glimps --version</Code>.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="history">Project history</H2>
        <P>
          <Glimps quiet /> is built on the proven PTY-supervisor model — the same approach used by{" "}
          <Code>script</Code>, <Code>tmux</Code>, and ChromaTerm. The PTY owner is what can
          safely observe and transform the output stream. Small <Code>preexec</Code>/
          <Code>precmd</Code>-style hooks complement that supervisor by reporting trusted
          command text, working directory, pipeline statuses, and exit code; they do not
          transform output themselves.
        </P>
        <P>
          It ships as a single native Rust binary — no interpreter or runtime to install
          (no Node, Python, or JVM). <Glimps quiet /> is currently in beta: macOS with zsh
          or bash today, with Linux as a supported build target, following a versioned
          roadmap from v0.1 toward v2.0.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="license">License & credits</H2>
        <P>
          <Glimps quiet /> is released under the MIT license. It draws on ideas and prior art from the
          long history of terminal tooling — from <Code>less</Code> and <Code>tput</Code>{" "}
          to modern pretty-printers like <Code>jq</Code>, <Code>bat</Code>, and{" "}
          <Code>delta</Code>, and to the PTY-supervisor lineage of ChromaTerm. <Glimps quiet /> does
          its own formatting; the debt to these tools is one of ideas, not integration.
        </P>
      </section>
    </DocsLayout>
  );
}

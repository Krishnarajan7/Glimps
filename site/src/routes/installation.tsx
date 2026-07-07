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
import { canonical } from "@/lib/seo";
import { Glimps } from "@/components/ui/glimps";

export const Route = createFileRoute("/installation")({
  head: () => ({
    meta: [
      { title: "Installation — GLIMPS" },
      {
        name: "description",
        content:
          "Install GLIMPS on macOS or Linux by building from source with Rust and Cargo. Enable it in bash or zsh, verify the install, upgrade, and uninstall — with no surprises.",
      },
      { property: "og:title", content: "Install GLIMPS" },
      {
        property: "og:description",
        content:
          "Build GLIMPS from source on macOS and Linux, enable it in bash or zsh, and try it without touching your shell.",
      },
      { property: "og:url", content: canonical("/installation") },
    ],
    links: [{ rel: "canonical", href: canonical("/installation") }],
  }),
  component: InstallPage,
});

const toc: TocItem[] = [
  { id: "requirements", label: "Requirements" },
  { id: "install", label: "Install" },
  { id: "from-source", label: "From source", depth: 2 },
  { id: "macos", label: "macOS", depth: 2 },
  { id: "linux", label: "Linux", depth: 2 },
  { id: "enable", label: "Enable in your shell" },
  { id: "zsh", label: "zsh", depth: 2 },
  { id: "bash", label: "bash", depth: 2 },
  { id: "verify", label: "Verify it works" },
  { id: "demo", label: "Try without installing" },
  { id: "upgrade", label: "Upgrade" },
  { id: "uninstall", label: "Uninstall" },
  { id: "troubleshooting", label: "Troubleshooting" },
];

function InstallPage() {
  return (
    <DocsLayout
      section="Install"
      title="Installation"
      intro={
        <>
          A <Glimps quiet /> install is two things: a single binary on your <Code>PATH</Code>, and
          one line in your shell startup file. Nothing else — no config, no daemon, no
          reboot. Today you build that binary from source with Rust.
        </>
      }
      toc={toc}
    >
      <section className="space-y-4">
        <H2 id="requirements">Requirements</H2>
        <UL>
          <li>macOS (Apple Silicon or Intel), or any recent Linux distribution.</li>
          <li>
            An interactive shell: zsh, or bash 3.2+ (the macOS system bash works).
          </li>
          <li>
            A Rust toolchain (<Code>rustc</Code> + <Code>cargo</Code>) to build from
            source — install it from{" "}
            <a href="https://rustup.rs" className="underline">
              rustup.rs
            </a>
            .
          </li>
          <li>
            Any terminal with basic ANSI color — GLIMPS emits standard 16-color output,
            so nothing special is required.
          </li>
        </UL>
      </section>

      <section className="space-y-6">
        <H2 id="install">Install</H2>

        <div className="space-y-3">
          <H3 id="from-source">From source</H3>
          <P>
            Building from source is the only supported path today. It needs a Rust
            toolchain (see requirements). Clone the repo and let Cargo build and install
            the <Code>glimps</Code> binary onto your <Code>PATH</Code>:
          </P>
          <Shell
            lines={[
              { cmd: "git clone https://github.com/Krishnarajan7/Glimps" },
              { cmd: "cd Glimps" },
              { cmd: "cargo install --path ." },
            ]}
          />
        </div>

        <div className="space-y-3">
          <H3 id="macos">macOS</H3>
          <P>
            Apple Silicon and Intel are identical — the same <Code>cargo install --path .</Code>{" "}
            from a checkout, no arch-specific steps.
          </P>
        </div>

        <div className="space-y-3">
          <H3 id="linux">Linux</H3>
          <P>
            Same as above: run <Code>cargo install --path .</Code> from a checkout of the
            repo.
          </P>
        </div>

        <Callout title="not shipped yet">
          These are planned but not available — don't try them yet:{" "}
          <Code>brew install glimps</Code>, <Code>cargo install glimps</Code> from
          crates.io, <Code>apt</Code>/<Code>dnf</Code>/<Code>pacman</Code> packages, and
          fish shell integration. Build from source for now.
        </Callout>
      </section>

      <section className="space-y-6">
        <H2 id="enable">Enable in your shell</H2>
        <P>
          <Glimps quiet /> prints a small init script per shell. Add one guarded line to the
          appropriate startup file and open a new terminal.
        </P>

        <Callout title="put it near the top">
          The init snippet re-execs your shell inside <Glimps quiet />, and the inner shell
          re-sources the same startup file. Place the line high up — after your critical{" "}
          <Code>PATH</Code>/env setup but before plugin managers and prompt frameworks —
          so those don't run twice per session. It never touches your prompt.
        </Callout>

        <div className="space-y-3">
          <H3 id="zsh">zsh</H3>
          <P>
            Add near the top of <Code>~/.zshrc</Code>:
          </P>
          <Shell
            lines={[
              {
                cmd: 'command -v glimps >/dev/null 2>&1 && eval "$(glimps init zsh)"',
              },
            ]}
          />
        </div>

        <div className="space-y-3">
          <H3 id="bash">bash</H3>
          <P>
            Add near the top of <Code>~/.bashrc</Code>. GLIMPS re-execs an{" "}
            <em>interactive, non-login</em> shell, which sources <Code>~/.bashrc</Code> —
            so on macOS, if your <Code>~/.bash_profile</Code> doesn't already source{" "}
            <Code>~/.bashrc</Code>, add the line to <Code>~/.bashrc</Code> anyway (not just{" "}
            <Code>~/.bash_profile</Code>), or the hook won't load.
          </P>
          <Shell
            lines={[
              {
                cmd: 'command -v glimps >/dev/null 2>&1 && eval "$(glimps init bash)"',
              },
            ]}
          />
        </div>

        <Callout title="the guarded form">
          The <Code>command -v glimps</Code> check keeps your shell startup working even
          if the binary is missing (fresh machine, restored dotfiles, container). If
          <Glimps quiet /> isn't installed, the line is a no-op. fish integration is planned but
          not available yet.
        </Callout>
      </section>

      <section className="space-y-4">
        <H2 id="verify">Verify it works</H2>
        <P>Confirm the binary is on your PATH:</P>
        <Shell
          lines={[{ cmd: "glimps --version" }, { out: "glimps 0.0.1" }]}
        />
        <P>
          Then open a new terminal, run any command, and confirm a <Code>▌</Code> command
          header appears above its output. If it doesn't, jump to{" "}
          <a href="#troubleshooting" className="underline">
            troubleshooting
          </a>
          .
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="demo">Try without installing</H2>
        <P>
          You can dogfood <Glimps quiet /> straight from a checkout without installing it globally
          or editing <Code>~/.zshrc</Code>. On macOS, this builds{" "}
          <Code>target/debug/glimps</Code> and wraps a throwaway zsh via a temporary{" "}
          <Code>ZDOTDIR</Code>, cleaning everything up on exit:
        </P>
        <Shell lines={[{ cmd: "scripts/dogfood-macos.sh session" }]} />
        <P>
          Once <Glimps quiet /> is installed, you can also just run <Code>glimps</Code> to start a
          wrapped shell and <Code>exit</Code> to leave it.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="upgrade">Upgrade</H2>
        <P>
          Because <Glimps quiet /> is built from source, upgrading means rebuilding. From your
          checkout, pull the latest changes and reinstall:
        </P>
        <Shell
          lines={[{ cmd: "git pull" }, { cmd: "cargo install --path ." }]}
        />
      </section>

      <section className="space-y-4">
        <H2 id="uninstall">Uninstall</H2>
        <P>Three steps:</P>
        <UL>
          <li>
            Remove the init line from your startup file:{" "}
            <Code>sed -i '' '/glimps init/d' ~/.zshrc</Code> (or <Code>~/.bashrc</Code>).
          </li>
          <li>
            Remove the binary: <Code>cargo uninstall glimps</Code> (or delete it from
            your <Code>PATH</Code>).
          </li>
          <li>
            Optionally delete your config at <Code>~/.glimpsrc</Code>.
          </li>
        </UL>
      </section>

      <section className="space-y-4">
        <H2 id="troubleshooting">Troubleshooting</H2>

        <H3 id="ts-no-bar">The ▌ bar doesn't appear</H3>
        <P>
          Work through these checks: confirm the binary is found (
          <Code>command -v glimps</Code> should print a path); confirm your startup file
          actually eval's <Code>glimps init</Code> (the guarded line is present near the
          top of the right file for your shell); and confirm you opened a{" "}
          <em>new</em> terminal after saving so the init runs.
        </P>

        <H3 id="ts-colors">Colors look wrong</H3>
        <P>
          GLIMPS uses standard 16-color ANSI, so the exact hues come from your terminal's
          color theme — adjust the palette there if a color reads badly. If you'd rather
          skip color entirely, set <Code>color = false</Code> in <Code>~/.glimpsrc</Code>{" "}
          for a structure-only, no-color mode.
        </P>

        <H3 id="ts-disable">Turn it off temporarily</H3>
        <P>
          Because <Glimps quiet /> wraps the whole shell, the switch is per-shell, not per-command.
          Start a raw shell with <Code>GLIMPS=0 zsh</Code>, keep future shells raw with{" "}
          <Code>export GLIMPS=0</Code>, or set <Code>enabled = false</Code> in{" "}
          <Code>~/.glimpsrc</Code> to turn it off persistently.
        </P>
      </section>
    </DocsLayout>
  );
}

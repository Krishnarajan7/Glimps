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
          "Install GLIMPS with Homebrew, the shell installer, crates.io, or from source on macOS, Linux, and Windows. Enable it in bash, zsh, or PowerShell, verify the install, upgrade, and uninstall — with no surprises.",
      },
      { property: "og:title", content: "Install GLIMPS" },
      {
        property: "og:description",
        content:
          "Install GLIMPS with Homebrew, a one-line installer, or Cargo, enable it in bash, zsh, or PowerShell, and try it without touching your shell.",
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
  { id: "homebrew", label: "Homebrew", depth: 2 },
  { id: "shell-installer", label: "Shell installer", depth: 2 },
  { id: "crates-io", label: "crates.io", depth: 2 },
  { id: "from-source", label: "From source", depth: 2 },
  { id: "windows", label: "Windows", depth: 2 },
  { id: "enable", label: "Enable in your shell" },
  { id: "zsh", label: "zsh", depth: 2 },
  { id: "bash", label: "bash", depth: 2 },
  { id: "powershell", label: "PowerShell", depth: 2 },
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
          reboot. Get the binary from Homebrew, a one-line installer, crates.io, or a
          source build.
        </>
      }
      toc={toc}
    >
      <section className="space-y-4">
        <H2 id="requirements">Requirements</H2>
        <UL>
          <li>
            macOS (Apple Silicon is the primary beta path; Intel artifacts are
            configured but clean-machine verification is pending), a recent Linux
            distribution, or Windows 10 1809+ / Windows 11 (experimental — see{" "}
            <a href="#windows" className="underline">
              Windows
            </a>
            ).
          </li>
          <li>
            An interactive shell: zsh, or bash 3.2+ (the macOS system bash works). On
            Windows: PowerShell 7 (<Code>pwsh</Code>) or Windows PowerShell 5.1.
          </li>
          <li>
            For the crates.io, source, and Windows installs only: a Rust toolchain (
            <Code>rustc</Code> + <Code>cargo</Code>) — install it from{" "}
            <a href="https://rustup.rs" className="underline">
              rustup.rs
            </a>
            .
          </li>
          <li>
            A modern terminal with ANSI SGR color support. GLIMPS uses standard 256-color
            and truecolor sequences, with a no-color mode available.
          </li>
        </UL>
      </section>

      <section className="space-y-6">
        <H2 id="install">Install</H2>

        <div className="space-y-3">
          <H3 id="homebrew">Homebrew (macOS / Linux)</H3>
          <P>
            The easiest path on macOS and Linuxbrew. Installs a prebuilt binary for Apple
            Silicon or Intel:
          </P>
          <Shell lines={[{ cmd: "brew install Krishnarajan7/tap/glimps" }]} />
        </div>

        <div className="space-y-3">
          <H3 id="shell-installer">Shell installer (macOS / Linux)</H3>
          <P>
            Downloads the prebuilt binary for your platform (Apple Silicon or Intel, macOS
            or Linux) from the latest GitHub release. Release artifacts carry{" "}
            <a
              href="https://github.com/Krishnarajan7/Glimps/attestations"
              className="underline"
              target="_blank"
              rel="noopener noreferrer"
            >
              build provenance attestations
            </a>
            .
          </P>
          <Shell
            lines={[
              {
                cmd: "curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Krishnarajan7/Glimps/releases/latest/download/glimps-installer.sh | sh",
              },
            ]}
          />
        </div>

        <div className="space-y-3">
          <H3 id="crates-io">crates.io</H3>
          <P>
            If you have a Rust toolchain, Cargo can build and install the published crate
            onto your <Code>PATH</Code>:
          </P>
          <Shell lines={[{ cmd: "cargo install glimps" }]} />
        </div>

        <div className="space-y-3">
          <H3 id="from-source">From source</H3>
          <P>
            To run the latest <Code>main</Code>, clone the repo and let Cargo build and
            install the <Code>glimps</Code> binary:
          </P>
          <Shell
            lines={[
              { cmd: "git clone https://github.com/Krishnarajan7/Glimps" },
              { cmd: "cd Glimps" },
              { cmd: "cargo install --path ." },
            ]}
          />
          <P>
            Every path produces the same single native binary. Apple Silicon is the
            primary beta path; physical Intel verification is still pending.
          </P>
        </div>

        <div className="space-y-3">
          <H3 id="windows">Windows</H3>
          <P>
            <Glimps quiet /> runs on Windows 10 1809+ and Windows 11, supervising PowerShell
            through ConPTY. Install Rust with the MSVC toolchain (
            <Code>stable-x86_64-pc-windows-msvc</Code>, the rustup default), which needs the
            Visual Studio Build Tools &quot;Desktop development with C++&quot; workload.
            There is no prebuilt Windows binary yet, and the crates.io v0.1.0 release
            predates Windows support, so build from a checkout in PowerShell:
          </P>
          <Shell
            lines={[
              { cmd: "git clone https://github.com/Krishnarajan7/Glimps" },
              { cmd: "cd Glimps" },
              { cmd: "cargo install --path ." },
            ]}
          />
          <P>
            This puts <Code>glimps.exe</Code> in <Code>%USERPROFILE%\.cargo\bin</Code>, which
            rustup already adds to your <Code>PATH</Code>. Windows Terminal is the
            recommended host.
          </P>
          <Callout title="experimental">
            Windows support is newer than macOS and Linux. The formatters are identical on
            every platform, but the ConPTY layer has had far less real-world use. If
            anything looks off, <Code>$env:GLIMPS=&apos;0&apos;</Code> turns it off instantly —
            and please{" "}
            <a href="/feedback" className="underline">
              report it
            </a>
            . <Code>cmd.exe</Code> has no shell hooks and only ever gets pass-through; WSL
            is Linux and uses the zsh/bash integration. Full notes live in{" "}
            <a
              href="https://github.com/Krishnarajan7/Glimps/blob/main/docs/windows.md"
              className="underline"
              target="_blank"
              rel="noopener noreferrer"
            >
              docs/windows.md
            </a>
            .
          </Callout>
        </div>

        <Callout title="not shipped yet">
          These are planned but not available yet: <Code>apt</Code>/<Code>dnf</Code>/
          <Code>pacman</Code> packages, prebuilt Windows binaries, and fish shell
          integration.
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

        <div className="space-y-3">
          <H3 id="powershell">PowerShell (Windows)</H3>
          <P>
            Open your profile with <Code>notepad $PROFILE</Code> and add this near the top:
          </P>
          <Shell
            lines={[
              {
                cmd: "if (Get-Command glimps -ErrorAction SilentlyContinue) { glimps init pwsh | Out-String | Invoke-Expression }",
              },
            ]}
          />
          <P>
            <Code>glimps setup</Code> prints this line rather than editing{" "}
            <Code>$PROFILE</Code> itself, because Documents may be OneDrive-redirected and
            only PowerShell knows the real path. PowerShell has no <Code>exec</Code>, so the
            outer shell waits for the <Glimps quiet /> session and exits with its status. To
            avoid the double startup, point a Windows Terminal profile at{" "}
            <Code>glimps.exe --shell pwsh</Code> instead.
          </P>
        </div>

        <Callout title="the guarded form">
          The <Code>command -v glimps</Code> (or <Code>Get-Command glimps</Code>) check keeps your shell startup working even
          if the binary is missing (fresh machine, restored dotfiles, container). If
          <Glimps quiet /> isn't installed, the line is a no-op. fish integration is planned but
          not available yet.
        </Callout>
      </section>

      <section className="space-y-4">
        <H2 id="verify">Verify it works</H2>
        <P>Confirm the binary is on your PATH:</P>
        <Shell
          lines={[{ cmd: "glimps --version" }, { out: "glimps 0.2.0" }]}
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
          On Windows, the equivalent builds the checkout and starts a disposable
          GLIMPS-wrapped PowerShell with a temporary config. It never edits{" "}
          <Code>$PROFILE</Code>. From <Code>cmd.exe</Code>, use{" "}
          <Code>scripts\dogfood-windows.cmd session</Code> instead:
        </P>
        <Shell lines={[{ cmd: ".\\scripts\\dogfood-windows.ps1 session" }]} />
        <P>
          Once <Glimps quiet /> is installed, you can also just run <Code>glimps</Code> to start a
          wrapped shell and <Code>exit</Code> to leave it.
        </P>
      </section>

      <section className="space-y-4">
        <H2 id="upgrade">Upgrade</H2>
        <P>Upgrade the same way you installed:</P>
        <UL>
          <li>
            Homebrew: <Code>brew upgrade glimps</Code>
          </li>
          <li>Shell installer: re-run the install command above.</li>
          <li>
            crates.io: <Code>cargo install glimps</Code> (it replaces an older version)
          </li>
          <li>
            From source (and Windows): <Code>git pull</Code>, then{" "}
            <Code>cargo install --path .</Code>
          </li>
        </UL>
      </section>

      <section className="space-y-4">
        <H2 id="uninstall">Uninstall</H2>
        <P>Three steps:</P>
        <UL>
          <li>
            Remove the init line from your startup file:{" "}
            <Code>sed -i '' '/glimps init/d' ~/.zshrc</Code> (or <Code>~/.bashrc</Code>).
            On Windows, delete the <Code>glimps init pwsh</Code> line from{" "}
            <Code>$PROFILE</Code>.
          </li>
          <li>
            Remove the binary: <Code>brew uninstall glimps</Code> if you used Homebrew,
            otherwise <Code>cargo uninstall glimps</Code> (or delete it from your{" "}
            <Code>PATH</Code>).
          </li>
          <li>
            Optionally delete your config at <Code>~/.glimpsrc</Code> (
            <Code>%USERPROFILE%\.glimpsrc</Code> on Windows).
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
          GLIMPS uses ANSI 256-color and truecolor sequences, so use a modern terminal with
          color support and check any terminal-level color overrides. If you'd rather skip
          color entirely, set <Code>color = false</Code> in <Code>~/.glimpsrc</Code> for a
          structure-only, no-color mode.
        </P>

        <H3 id="ts-disable">Turn it off temporarily</H3>
        <P>
          Because <Glimps quiet /> wraps the whole shell, the switch is per-shell, not per-command.
          Start a raw shell with <Code>GLIMPS=0 zsh</Code>, keep future shells raw with{" "}
          <Code>export GLIMPS=0</Code> (in PowerShell: <Code>$env:GLIMPS=&apos;0&apos;</Code>), or set <Code>enabled = false</Code> in{" "}
          <Code>~/.glimpsrc</Code> to turn it off persistently.
        </P>
      </section>
    </DocsLayout>
  );
}

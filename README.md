# Abito

Abito is a small desktop island for your coding agents. It sits at the top of the screen, shows what **Claude Code**, **Codex**
and **Pi** are doing, and lets you answer their questions and permission requests without switching to the terminal.

> **Status:** early preview. This version works on **Chat** and **Agentes**; dictation, «OK Abito», integrations, Spotify
> and the phone link are paused and hidden (see [docs/paused-features.md](docs/paused-features.md)).

## Download

Get the latest build from **[Releases](https://github.com/Alvarogds04/abito/releases)**:

| Platform | File | Notes |
| --- | --- | --- |
| Linux x86_64 | `abito-linux-x64.tar.gz` | `tar -xzf abito-linux-x64.tar.gz && sh abito-linux-x64/install.sh abito-linux-x64/abito-island --project-root ~/your-project` |
| macOS (Apple Silicon) | `Abito-macos-arm64.dmg` / `.zip` | **not notarized**: open it once, then *System Settings → Privacy & Security → Open Anyway*. Step-by-step guide (Spanish): [docs/mac-guide.es.md](docs/mac-guide.es.md) |
| Windows x64 | `abito-windows-x64-setup.exe` | **unsigned**: SmartScreen → *More info* → *Run anyway* |

The macOS build is ad-hoc signed but not notarized; the Windows build is not signed. They are meant for testing.

**Agents need [Node.js](https://nodejs.org).** Chat also needs Pi (`npm install -g @earendil-works/pi-coding-agent`);
Claude Code and Codex hooks do not. Without them the island still opens and shows notices, and Abito says which one is missing.

## Windows limitations

Automatic hook and Pi-extension installation from Settings is not available on Windows yet.
Affected rows show «No disponible en esta plataforma todavía» and do not offer installation.
The terminal installer shares the same filesystem limitations and is not a supported workaround.
Windows support requires verified audit creation, status-line side-file handling, and bundled-script trust checks.

## Connect your agents

- **Claude Code / Codex:** Abito installs their hooks from *Ajustes → Agentes*.
- **Pi:** install the Abito extension from *Ajustes → Agentes* (Linux and macOS), or load it by hand with
  `pi -e apps/desktop-pi/pi-extension.ts`. Pi's yes/no confirmations then also appear in the island (credential,
  token and consent prompts stay in the terminal).

Each agent row in *Ajustes → Agentes* works on its own: Claude Code and Codex hooks do not need Pi.

## Repository

| Folder | What it is |
| --- | --- |
| `apps/desktop-tauri` | the island (Tauri 2 + Rust, plain JS frontend) |
| `apps/desktop-daemon` | local daemon for chat, accounts and agent sessions (Node) |
| `apps/desktop-harness` | Claude Code / Codex hook handlers (Node) |
| `apps/desktop-pi` | Pi extension |
| `apps/desktop-sound` | sound engine |

Build on Linux: `cd apps/desktop-tauri && node packaging/build.cjs --out dist`. Tests: `../../run-node-checks.sh` (Node) and
`cargo test --offline`.

## License

MIT — see [LICENSE](LICENSE). Parts of the frontend are adapted from Coucou (MIT).

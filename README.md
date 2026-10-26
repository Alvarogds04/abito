# Abito

Abito is a small desktop island for your coding agents. It sits at the top of the screen, shows what **Claude Code**, **Codex**
and **Pi** are doing, and lets you answer their questions and permission requests without switching to the terminal.

> **Status:** early preview. This version works on **Chat** and **Agentes**; dictation, «OK Abito», integrations, Spotify
> and the phone link are visible but paused (see [docs/paused-features.md](docs/paused-features.md)).

## Download

Get the latest build from **[Releases](https://github.com/Alvarogds04/abito/releases)**:

| Platform | File | Notes |
| --- | --- | --- |
| Linux x86_64 | `abito-linux-x64.tar.gz` | `tar -xzf abito-linux-x64.tar.gz && sh abito-linux-x64/install.sh abito-linux-x64/abito-island --project-root ~/your-project` |
| macOS (Apple Silicon) | `Abito-macos-arm64.dmg` / `.zip` | **unsigned**: first open with right click → *Open*, or `xattr -dr com.apple.quarantine /Applications/Abito.app` |
| Windows x64 | `abito-windows-x64-setup.exe` | **unsigned**: SmartScreen → *More info* → *Run anyway* |

The macOS and Windows builds are not signed or notarized. They are meant for testing.

## Connect your agents

- **Claude Code / Codex:** Abito installs their hooks from *Ajustes → Agentes*.
- **Pi:** load the extension in `apps/desktop-pi` (`pi -e apps/desktop-pi/pi-extension.ts`, or copy it to
  `~/.pi/agent/extensions/abito-island/`).

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

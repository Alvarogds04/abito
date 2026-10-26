'use strict';
// JSON passed to tauri build --config. Paths are relative to the Tauri crate,
// not this script's cwd. Reuse the Linux packager's closed, tested source lists.
const { DAEMON_FILES, HARNESS_FILES } = require('./lib.cjs');
process.stdout.write(JSON.stringify({
  bundle: {
    active: true,
    icon: ['icons/icon-macos.png'],
    category: 'Productivity',
    macOS: { minimumSystemVersion: '12.0' },
    resources: Object.fromEntries([
      ...DAEMON_FILES.map(file => [`../desktop-daemon/${file}`, `desktop-daemon/${file}`]),
      ...HARNESS_FILES.map(file => [`../desktop-harness/${file}`, `desktop-harness/${file}`]),
    ]),
  },
}) + '\n');

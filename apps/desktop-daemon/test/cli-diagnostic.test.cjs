"use strict";
const assert = require("node:assert/strict");
const test = require("node:test");
const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

test("CLI startup diagnostics name failures without paths or secrets", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "abito-cli-diagnostic-"));
  try {
    for (const [message, code, expected] of [
      ["EACCES: permission denied, open C:\\Users\\private\\token-secret", "EACCES", "Error/EACCES: system operation failed"],
      ["refusing symlink component in island path: /private/token-secret", undefined, "Error/unknown: refusing symlink component in island path [details redacted]"],
      ["token-secret /private/account", "token-secret", "Error/unknown: details redacted"],
      ["daemon already running", undefined, null],
    ]) {
      const source = `
        const { Daemon } = require('./src/daemon.cjs');
        Daemon.prototype.start = async function () { throw Object.assign(new Error(${JSON.stringify(message)}), { code: ${JSON.stringify(code)} }); };
        Daemon.prototype.stop = async function () {};
        Daemon.prototype.cleanupSync = function () {};
        require('./src/cli.cjs').main(['run']).then(code => { process.exitCode = code; });
      `;
      const result = spawnSync(process.execPath, ["-e", source], {
        cwd: path.join(__dirname, ".."), encoding: "utf8",
        env: { ...process.env, LOCALAPPDATA: root, ABITO_ISLAND_DIR: path.join(root, "Abito") },
      });
      assert.equal(result.status, 1, result.stderr);
      assert.equal(result.stdout, "");
      assert.equal(result.stderr, expected === null
        ? "abito-island-daemon: daemon already running\n"
        : `abito-island-daemon: failed (see the island directory permissions and the daemon log) [${expected}]\n`);
      assert.deepEqual(fs.readdirSync(root), [], "rejected startup must not write state");
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

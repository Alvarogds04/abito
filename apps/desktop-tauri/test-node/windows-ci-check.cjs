#!/usr/bin/env node
'use strict';
// Structural policy only; native ParseFile and install smoke must still run in CI.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ci = path.join(__dirname, '../ci/public');
const workflow = fs.readFileSync(path.join(ci, 'preview-windows.yml'), 'utf8');
const firstStep = workflow.split('    steps:\n')[1].split('      - uses: actions/checkout@v4')[0];
assert.match(firstStep, /working-directory: \$\{\{ runner.temp \}\}/);
assert.match(firstStep, /if \(-not \$IsWindows -or \$env:GITHUB_ACTIONS -ne 'true' -or\s+\$env:RUNNER_ENVIRONMENT -ne 'github-hosted'\) \{\s+throw /);
assert.ok(firstStep.indexOf('throw ') < firstStep.indexOf('\n          }'));
assert.doesNotMatch(firstStep, /continue-on-error|choco|npm|uses:/);
const smoke = fs.readFileSync(path.join(ci, 'smoke-windows.ps1'), 'utf8');
assert.match(smoke.slice(0, smoke.indexOf('New-Item')), /if \(-not \$IsWindows -or \$env:GITHUB_ACTIONS -ne 'true' -or \$env:RUNNER_ENVIRONMENT -ne 'github-hosted'\) \{\s+throw /);
console.log('PASS: hosted-Windows guards precede workflow setup and smoke mutations (structural only)');

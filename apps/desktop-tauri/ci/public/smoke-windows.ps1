# Full installed-runtime smoke. Only the disposable GitHub-hosted Windows runner.
param([Parameter(Mandatory = $true)][string]$Setup)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows -or $env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    throw 'Installed smoke requires a disposable GitHub-hosted Windows runner'
}
$root = Join-Path $env:RUNNER_TEMP 'abito-installed-smoke'
if (Test-Path -LiteralPath $root) { throw 'Smoke root already exists; refusing to reuse state' }
New-Item -ItemType Directory -Path $root | Out-Null
$install = Join-Path $root 'installed'
$workspace = Join-Path $root 'workspace'
$nodeDir = Join-Path $root 'node'
$roaming = Join-Path $root 'roaming'
$empty = Join-Path $root 'empty'
foreach ($dir in @($workspace, $nodeDir, $roaming, $empty)) { New-Item -ItemType Directory -Path $dir | Out-Null }
$node = Join-Path $nodeDir 'node.exe'
Copy-Item -LiteralPath (Get-Command node.exe).Source -Destination $node
# Pi is a real package, isolated from the runner's global npm prefix. No model call.
& npm.cmd install --global --prefix (Join-Path $roaming 'npm') '@earendil-works/pi-coding-agent'
if ($LASTEXITCODE -ne 0) { throw 'Isolated Pi installation failed' }
$localData = [Environment]::GetFolderPath('LocalApplicationData')
$recap = Join-Path $localData 'com.abitog.AbitoTauri/recap.json'
if (Test-Path -LiteralPath $recap) { throw 'Runner already has island recap data; refusing smoke against existing state' }
if (Test-Path -LiteralPath (Join-Path $localData 'Abito')) { throw 'Runner already has island runtime data; refusing reuse' }
$originalEnv = @{}
foreach ($key in @('PATH', 'APPDATA', 'LOCALAPPDATA', 'ProgramFiles', 'ABITO_PROJECT_ROOT', 'ABITO_ISLAND_DIR', 'ABITO_DESKTOP_SOCKET')) {
    $originalEnv[$key] = [Environment]::GetEnvironmentVariable($key, 'Process')
}
$process = $null
function Show-SmokeDiagnostics {
    Write-Host 'Installed smoke diagnostics (no session/token/config contents)'
    if ($null -ne $script:process) {
        Write-Host "Island pid=$($script:process.Id), exited=$($script:process.HasExited)"
    }
    $logs = @(Get-ChildItem -LiteralPath $root -Filter '*.log' -File)
    foreach ($log in $logs) {
        Write-Host "--- $($log.Name) ---"
        Get-Content -LiteralPath $log.FullName -Tail 80 | ForEach-Object { Write-Host $_ }
    }
    Write-Host 'The app-owned daemon inherits island stderr; generic startup exceptions have a redacted diagnostic suffix.'
    foreach ($dir in @((Join-Path $env:LOCALAPPDATA 'Abito'), (Join-Path $env:LOCALAPPDATA 'Abito/runtime'), (Join-Path $env:LOCALAPPDATA 'Abito/notices'))) {
        if (Test-Path -LiteralPath $dir) {
            Write-Host "Directory: $dir"
            Get-ChildItem -LiteralPath $dir -Force | Select-Object Name,Length,Mode | Format-Table | Out-String | Write-Host
            try { Write-Host "ACL: $((Get-Acl -LiteralPath $dir).Sddl)" } catch { Write-Host 'ACL diagnostic unavailable' }
        } else { Write-Host "Directory absent: $dir" }
    }
    Write-Host 'No separate daemon.log/failure-record writer is used by the app-owned daemon; session.json is deliberately not printed.'
}
function Wait-For([scriptblock]$Condition, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(40)
    do {
        if (& $Condition) { return }
        if ($null -ne $script:process -and $script:process.HasExited) { throw "Island exited while waiting for $Description" }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $Description"
}
function Stop-Island {
    if ($null -ne $script:process) {
        if (-not $script:process.HasExited) { $script:process.Kill($true) }
        if (-not $script:process.WaitForExit(10000)) { throw 'Owned island did not exit' }
        $script:process.Dispose()
        $script:process = $null
    }
}
function Start-Island([string]$Mode) {
    # The ready scenario must prove the real default profile ACL works. Never
    # rewrite it to make the test pass. Missing-runtime scenarios use fresh roots.
    $ready = $Mode -eq 'ready' -or $Mode -eq 'explicit-root'
    $data = if ($ready) { $localData } else { Join-Path $root $Mode }
    if (-not $ready) {
        New-Item -ItemType Directory -Path $data | Out-Null
        $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
        $acl = [System.Security.AccessControl.DirectorySecurity]::new()
        $acl.SetOwner($sid)
        $acl.SetAccessRuleProtection($true, $false)
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', 'ContainerInherit, ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
        Set-Acl -LiteralPath $data -AclObject $acl
    }
    $env:LOCALAPPDATA = $data
    $env:APPDATA = if ($ready) { $roaming } else { $empty }
    $env:ProgramFiles = $empty
    $systemPath = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:PATH = if ($Mode -eq 'no-node') { $systemPath } else { "$nodeDir;$systemPath" }
    if ($Mode -eq 'explicit-root') { $env:ABITO_PROJECT_ROOT = $workspace }
    else { Remove-Item Env:ABITO_PROJECT_ROOT -ErrorAction SilentlyContinue }
    Remove-Item Env:ABITO_ISLAND_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:ABITO_DESKTOP_SOCKET -ErrorAction SilentlyContinue
    $script:process = Start-Process -FilePath (Join-Path $install 'abito-island.exe') -WorkingDirectory $workspace -PassThru `
        -RedirectStandardOutput (Join-Path $root "$Mode.stdout.log") -RedirectStandardError (Join-Path $root "$Mode.stderr.log")
    # A forced stop can leave the previous process's metadata. Wait for this
    # launch to publish its own record; existence alone is not readiness.
    Wait-For {
        try {
            $candidate = Get-Content -Raw -LiteralPath (Join-Path $data 'Abito/notices/endpoint.json') | ConvertFrom-Json
            return $candidate.pid -eq $script:process.Id
        } catch { return $false }
    } "$Mode receiver metadata"
    $metadata = Get-Content -Raw -LiteralPath (Join-Path $data 'Abito/notices/endpoint.json') | ConvertFrom-Json
    if ($metadata.pid -ne $script:process.Id) { throw 'Receiver metadata does not belong to installed island process' }
}
try {
    $setupProcess = Start-Process -FilePath (Resolve-Path -LiteralPath $Setup).Path -ArgumentList @('/S', "/D=$install") -PassThru -Wait
    if ($setupProcess.ExitCode -ne 0) { throw "Installer failed: $($setupProcess.ExitCode)" }
    # Exact file set from the same closed lists as staging, without a parallel payload list.
    $expectedJson = & $node -e 'const s=require("./packaging/windows/stage-runtime.cjs"); console.log(JSON.stringify([...s.BINARY_FILES,...s.RUNTIME_FILES,"uninstall.exe"].sort()))'
    if ($LASTEXITCODE -ne 0) { throw 'Cannot load expected installed file set' }
    $expected = @($expectedJson | ConvertFrom-Json)
    $actual = @(Get-ChildItem -LiteralPath $install -File -Recurse | ForEach-Object { [IO.Path]::GetRelativePath($install, $_.FullName).Replace('\', '/') } | Sort-Object)
    if (@(Compare-Object $expected $actual).Count -ne 0) { throw 'Installed payload differs from closed file set' }
    foreach ($file in $expected) {
        if ($file -eq 'uninstall.exe') { continue }
        $source = if ($file.EndsWith('.exe')) { Join-Path $PWD "dist/$file" } else { Join-Path $PWD "dist/windows-runtime/$file" }
        if ((Get-FileHash -LiteralPath $source).Hash -ne (Get-FileHash -LiteralPath (Join-Path $install $file)).Hash) { throw "Installed hash mismatch: $file" }
    }
    Start-Island 'ready'
    Wait-For { (Get-Content -Raw -LiteralPath (Join-Path $root 'ready.stderr.log')) -match 'abito-island-daemon: running' } 'installed daemon startup'
    # Public installed daemon status performs the native authenticated session checks.
    & $node (Join-Path $install 'desktop-daemon/src/cli.cjs') status
    if ($LASTEXITCODE -ne 0) { throw 'Installed daemon status refused' }
    $payload = @{ hook_event_name = 'Stop'; cwd = $workspace; session_id = 'ci4-installed-stop'; stop_hook_active = $false } | ConvertTo-Json -Compress
    $hookOutput = $payload | & $node (Join-Path $install 'desktop-harness/bin/abito-hook.cjs') claude stop 2> (Join-Path $root 'hook.stderr.log')
    if ($LASTEXITCODE -ne 0 -or $hookOutput -or (Get-Item -LiteralPath (Join-Path $root 'hook.stderr.log')).Length -ne 0) { throw 'Installed hook contract failed' }
    # Only the island's accepted settled callback writes this turn. CLI exit alone is NOT proof.
    # Tauri's Windows Known Folder lookup may use the actual profile or the child LOCALAPPDATA.
    $recapCandidates = @($recap, (Join-Path $env:LOCALAPPDATA 'com.abitog.AbitoTauri/recap.json')) | Select-Object -Unique
    Wait-For {
        foreach ($candidate in $recapCandidates) {
            if (Test-Path -LiteralPath $candidate) {
                $history = Get-Content -Raw -LiteralPath $candidate | ConvertFrom-Json
                if (@($history.turns).Count -eq 1) { $script:acceptedRecap = $candidate; return $true }
            }
        }
        return $false
    } 'island persisted installed hook receipt'
    Write-Host 'PASS installed hook -> installed authenticated relay -> receiver -> island persisted turn'
    Stop-Island
    Start-Island 'explicit-root'
    Wait-For { (Get-Content -Raw -LiteralPath (Join-Path $root 'explicit-root.stderr.log')) -match 'abito-island-daemon: running' } 'explicit Windows root daemon startup'
    Write-Host 'PASS installed receiver and daemon with explicit Windows project root'
    Stop-Island
    foreach ($case in @(
        @{ mode = 'no-pi'; message = 'Falta Pi: instalalo con npm install -g @earendil-works/pi-coding-agent.' },
        @{ mode = 'no-node'; message = 'Falta Node.js: instalalo desde nodejs.org.' }
    )) {
        Start-Island $case.mode
        Wait-For { (Get-Content -Raw -LiteralPath (Join-Path $root "$($case.mode).stderr.log")) -like "*$($case.message)*" } $case.message
        if ((Get-Content -Raw -LiteralPath (Join-Path $root "$($case.mode).stderr.log")) -match 'abito-island-daemon: running') { throw 'Daemon unexpectedly started without runtime' }
        Write-Host "PASS installed missing-runtime diagnostic: $($case.message)"
        Stop-Island
    }
    $unrelated = Join-Path $install 'desktop-daemon/src/user-note.txt'
    [IO.File]::WriteAllText($unrelated, 'must survive uninstall')
    $recapHash = (Get-FileHash -LiteralPath $script:acceptedRecap).Hash
    $uninstaller = Start-Process -FilePath (Join-Path $install 'uninstall.exe') -ArgumentList '/S' -PassThru -Wait
    if ($uninstaller.ExitCode -ne 0) { throw "Uninstaller failed: $($uninstaller.ExitCode)" }
    Wait-For { -not (Test-Path -LiteralPath (Join-Path $install 'uninstall.exe')) } 'uninstaller completion'
    foreach ($file in $expected) { if (Test-Path -LiteralPath (Join-Path $install $file)) { throw "Uninstall left shipped file: $file" } }
    if ([IO.File]::ReadAllText($unrelated) -ne 'must survive uninstall') { throw 'Uninstall changed unrelated file' }
    if ((Get-FileHash -LiteralPath $script:acceptedRecap).Hash -ne $recapHash) { throw 'Uninstall changed island user data' }
    Write-Host 'PASS full install, native runtime, missing runtimes and exact preserving uninstall'
} catch {
    Show-SmokeDiagnostics
    throw
} finally {
    Stop-Island
    foreach ($key in $originalEnv.Keys) { [Environment]::SetEnvironmentVariable($key, $originalEnv[$key], 'Process') }
    # Keep disposable runner logs/data for diagnostics. No recursive user-profile cleanup.
}

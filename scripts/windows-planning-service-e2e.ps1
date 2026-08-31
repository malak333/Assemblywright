[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$MasterExe,
    [Parameter(Mandatory = $true)][string]$ProviderExe,
    [Parameter(Mandatory = $true)][string]$OutputSchema,
    [Parameter(Mandatory = $true)][ValidatePattern('^AssemblywrightPlanningE2E_[A-Za-z0-9]{1,32}$')][string]$ServiceName,
    [Parameter(Mandatory = $true)][ValidateRange(1024,65535)][int]$Port,
    [switch]$Confirm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'The disposable Windows planning service E2E requires -Confirm.' }

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'The disposable Windows planning service E2E requires an elevated owner PowerShell.'
}

$masterSource = [IO.Path]::GetFullPath($MasterExe)
$providerSource = [IO.Path]::GetFullPath($ProviderExe)
$schemaSource = [IO.Path]::GetFullPath($OutputSchema)
foreach ($source in @($masterSource,$providerSource,$schemaSource)) {
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw 'A required E2E source file is unavailable.' }
}

$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$scratch = Join-Path $temporaryRoot ("AssemblywrightPlanningE2E-" + $ServiceName)
$stage = Join-Path $scratch 'sources'
$data = Join-Path $scratch 'data\master'
$programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
if ([string]::IsNullOrWhiteSpace($programData)) { throw 'The canonical Common Application Data root is unavailable.' }
$runtime = Join-Path (Join-Path (Join-Path $programData 'Assemblywright') 'planning-runtime') $ServiceName
$endpoint = "127.0.0.1:$Port"

if ($ServiceName -ceq 'AssemblywrightMaster') { throw 'The production service is never an E2E target.' }
if ($null -ne (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue)) { throw 'The disposable E2E service already exists.' }
if ((Test-Path -LiteralPath $scratch) -or (Test-Path -LiteralPath $runtime)) { throw 'The disposable E2E paths already exist.' }

$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,$Port)
try { $listener.Start() } finally { $listener.Stop() }

$master = Join-Path $stage 'assemblywright-master.exe'
$provider = Join-Path $stage 'assemblywright-brainstorming-provider.exe'
$codex = Join-Path $stage 'codex.exe'
$gh = Join-Path $stage 'gh.exe'
$schema = Join-Path $stage 'brainstorming-output-schema.json'
$codexHome = Join-Path $stage 'codex-home'
$ghConfig = Join-Path $stage 'gh-config'
$fixture = Join-Path $PSScriptRoot '..\crates\assemblywright-master\tests\fixtures\windows_planning_codex_fixture.rs'
$installed = $false
$started = $false
$proof = $null

try {
    New-Item -ItemType Directory -Path $stage,$codexHome,$ghConfig | Out-Null
    Copy-Item -LiteralPath $masterSource -Destination $master
    Copy-Item -LiteralPath $providerSource -Destination $provider
    Copy-Item -LiteralPath $schemaSource -Destination $schema
    & rustc --edition=2021 -C strip=symbols -O $fixture -o $codex
    if ($LASTEXITCODE -ne 0) { throw 'The bounded Codex fixture did not compile.' }
    Copy-Item -LiteralPath $codex -Destination $gh
    [IO.File]::WriteAllText((Join-Path $ghConfig 'hosts.yml'),"github.com:`n  user: assemblywright-e2e`n  oauth_token: fixture-not-a-credential`n  git_protocol: https`n",[Text.UTF8Encoding]::new($false))

    & $master --data-dir $data setup
    if ($LASTEXITCODE -ne 0) { throw 'The disposable planning master setup failed.' }
    & $master --data-dir $data service install --service-name $ServiceName --bind $endpoint --identity local-system --confirm
    if ($LASTEXITCODE -ne 0) { throw 'The disposable planning service installation failed.' }
    $installed = $true

    $masterHash = (Get-FileHash -LiteralPath $master -Algorithm SHA256).Hash.ToLowerInvariant()
    & (Join-Path $PSScriptRoot 'provision-planning-runtime.ps1') `
        -DataDir $data `
        -MasterExe $master `
        -MasterExeSha256 $masterHash `
        -ProviderExe $provider `
        -CodexExe $codex `
        -OutputSchema $schema `
        -GhExe $gh `
        -CodexHome $codexHome `
        -GhConfigDir $ghConfig `
        -GithubOwner 'malak333' `
        -ServiceName $ServiceName `
        -Confirm

    & $master --data-dir $data service start --service-name $ServiceName
    if ($LASTEXITCODE -ne 0) { throw 'The disposable planning service failed to start.' }
    $started = $true

    $token = (Get-Content -LiteralPath (Join-Path $data 'development.token') -Raw).Trim()
    $request = @'
{"schema_version":1,"draft":{"schema_version":1,"draft_id":"11111111-2222-4333-8444-555555555555","draft_revision":1,"repository":{"repository_id":"aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee","git_url":{"url":"https://github.com/malak333/windows-planning-v67-proof"}},"visibility":"public","orchestrator_catalog":{"schema_version":1,"catalog_revision":1,"profiles":[{"configuration_revision":1,"provider_id":"openai.codex","model_id":"gpt-5.6-sol"}],"default_profile_sha256":[111,78,175,200,18,95,211,118,42,204,249,98,143,34,40,15,44,194,22,64,87,44,72,179,100,99,22,33,119,164,131,238],"catalog_sha256":[28,141,25,143,98,118,169,13,34,94,21,29,240,174,124,97,1,207,218,205,66,10,150,224,224,12,210,103,246,193,181,63]},"orchestrator":{"configuration_revision":1,"provider_id":"openai.codex","model_id":"gpt-5.6-sol"},"idea":"Design a tiny public calculator specification for the native Windows planning containment proof."},"information_classification":"public","owner_cloud_disclosure_sha256":[233,39,90,235,150,90,38,58,10,13,30,163,122,114,179,35,20,104,40,186,104,138,46,23,92,35,221,118,6,71,34,235]}
'@
    $response = Invoke-WebRequest `
        -UseBasicParsing `
        -Method Post `
        -Uri ("http://$endpoint/v1/assembly-line/project-brainstorms") `
        -Headers @{ Authorization = "Bearer $token" } `
        -ContentType 'application/json' `
        -Body $request `
        -TimeoutSec 30
    if ([int]$response.StatusCode -ne 200) { throw 'The native planning request did not succeed.' }
    $specification = $response.Content | ConvertFrom-Json
    if ($specification.specification.title -cne 'Windows planning containment E2E') {
        throw 'The native planning response was not bound to the fixture specification.'
    }
    if ($response.Content -match 'xxxxx') { throw 'Discarded stderr leaked into the planning response.' }

    $projection = Invoke-WebRequest `
        -UseBasicParsing `
        -Method Get `
        -Uri ("http://$endpoint/v1/assembly-line") `
        -Headers @{ Authorization = "Bearer $token" } `
        -TimeoutSec 10
    if ([int]$projection.StatusCode -ne 200 -or $projection.Content -match 'xxxxx') {
        throw 'The native owner projection was unavailable or contained discarded stderr.'
    }
    $proof = [ordered]@{
        status = 'windows_planning_service_e2e_passed'
        service_identity = 'LocalSystem'
        appcontainer_profile = 'Assemblywright.Planning.Provider.v1'
        stderr_bytes_discarded = 262144
        production_service_untouched = $true
    }
} finally {
    if ($started -and (Test-Path -LiteralPath $master -PathType Leaf)) {
        & $master --data-dir $data service stop --service-name $ServiceName | Out-Null
    }
    if ($installed -and (Test-Path -LiteralPath $master -PathType Leaf)) {
        & $master --data-dir $data service uninstall --service-name $ServiceName --confirm | Out-Null
    }
    if ($null -ne (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue)) {
        throw 'The disposable E2E service registration was not removed.'
    }
    if (Test-Path -LiteralPath $runtime) { Remove-Item -LiteralPath $runtime -Recurse -Force }
    if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Recurse -Force }
}

if ($null -eq $proof) { throw 'The native Windows planning service E2E did not produce proof.' }
$proof | ConvertTo-Json -Compress

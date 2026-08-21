param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Status", "Check", "Admit", "SelfTest")]
    [string]$Action,
    [string]$ReceiptPath = "",
    [string]$DigestPath = "",
    [string]$DataDir = "",
    [switch]$Confirm
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$endpoint = "127.0.0.1:7791"
$schemaVersion = 1
$maxDigestBytes = 65
$shaPattern = "^[0-9a-f]{64}$"
$evidenceKeys = @(
    "repository_gate_proof", "restricted_worker_live", "review_provider_live",
    "github_publication_live", "restart_recovery_live",
    "mac_windows_control_event_streaming_live"
)

if ($DataDir.Length -eq 0) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw "The Windows-local master data directory is unavailable."
    }
    $DataDir = Join-Path $env:LOCALAPPDATA "Assemblywright\master"
}

$contracts = @{
    repository_gate_proof = [ordered]@{
        Origin = "repository_gate_proof_controller"; Schema = "assemblywright.repository-gate-proof.v1"
        GateIdentity = "assemblywright.release-local.v1"
        Boundary = "Exact clean main at origin/main ran the exact committed local-gate bytes with pre/post same-UID mutation edge checks, not host isolation; this is not activation admission, signing, notarization, live-device, restricted-worker, review-provider, GitHub-publication, restart-recovery, Mac/Windows-control, or production-readiness proof."
        Receipt = "repository-gate-proof.json"; Digest = "repository-gate-proof.sha256"; MaxBytes = 2048
        Keys = @("schema","category","origin","head_commit","tree_id","release_local_definition_sha256","gate_identity","observed_at_ms","status","proof_boundary")
    }
    restricted_worker_live = [ordered]@{
        Origin = "restricted_worker_proof_controller"; Schema = "assemblywright.restricted-worker-live-proof.v1"
        ProofIdentity = "assemblywright.restricted-worker-live.v1"
        Boundary = "One owner-supervised signed Swift relay and real Rust agent completed the exact protocol-v5 snapshot-bound restricted-worker attempt against the schema-v19 Windows master, including Windows artifact validation, isolated candidate verification, retained-pair observation, cancellation, abandonment, and cleanup; this same-owner live proof is not a host sandbox or OS-wide egress claim and is not activation admission, review-provider, GitHub-publication, restart-recovery, Mac/Windows-control-streaming, notarization, clean-profile, or production-readiness proof."
        Receipt = "restricted-worker-live-proof.json"; Digest = "restricted-worker-live-proof.sha256"; MaxBytes = 3072
        Keys = @("schema","category","origin","head_commit","tree_id","mac_live_harness_definition_sha256","windows_live_control_definition_sha256","proof_transcript_sha256","proof_identity","observed_at_ms","status","proof_boundary")
    }
    review_provider_live = [ordered]@{
        Origin = "review_provider_proof_controller"; Schema = "assemblywright.review-provider-live-proof.v1"
        Boundary = "Exact clean main at origin/main used the committed controller, harness, Windows control, adapter, and output-schema definitions to run one fixed approval and one fixed rejection through the selected pinned Codex adapter under the Windows master Job Object; this is semantic sanity proof, not activation evidence admission, general review competence, queue or gateway lifecycle, publication, restart recovery, control streaming, signing, notarization, or production-readiness proof."
        Receipt = "review-provider-live-proof.json"; Digest = "review-provider-live-proof.sha256"; MaxBytes = 4096
        Keys = @("schema","category","origin","head_commit","tree_id","controller_definition_sha256","harness_definition_sha256","windows_control_definition_sha256","adapter_definition_sha256","output_schema_definition_sha256","transcript_sha256","provider_id","model_id","observed_at_ms","status","proof_boundary")
    }
    github_publication_live = [ordered]@{
        Origin = "github_publication_proof_controller"; Schema = "assemblywright.github-publication-live-proof.v1"
        ProofIdentity = "assemblywright.github-publication-live.v1"
        Boundary = "Exact clean published main used the committed controller, harness, and Windows-control definitions to create one bounded metadata-only proof-marker pull request, require the two fixed protected checks, merge it, and reconcile origin/main to the reported protected merge commit; the local source checkout remained unchanged. This is fixed GitHub-publication integration proof, not activation-evidence admission, general branch-protection proof, queue lifecycle, restricted-worker, review-provider, restart-recovery, control-streaming, signing, notarization, or production-readiness proof."
        Receipt = "github-publication-live-proof.json"; Digest = "github-publication-live-proof.sha256"; MaxBytes = 4096
        Keys = @("schema","category","origin","source_head_commit","source_tree_id","published_main_commit","master_executable_sha256","controller_definition_sha256","harness_definition_sha256","windows_control_definition_sha256","proof_transcript_sha256","proof_identity","observed_at_ms","status","proof_boundary")
    }
    restart_recovery_live = [ordered]@{
        Origin = "restart_recovery_proof_controller"; Schema = "assemblywright.restart-recovery-live-proof.v2"
        ProofIdentity = "assemblywright.restart-recovery-live.v2"
        Boundary = "Exact clean published main used the committed controller, harness, and Windows-control definitions to prove real Rust-agent retained-workspace functional recovery plus idle schema-v19 authoritative Windows-master stopped-state recovery and exact restoration. It separately binds the original/restored service digest and the transient pinned-toolchain exact-source rebuild digest, plus pinned Mac Git and Cargo, Windows Cargo, rustc, MSVC, frozen-database, migration, and continuity digests, and proves distinct healthy PIDs. Repository native focused tests separately cover master startup quarantine. This is not reproducible-build, installed-image source provenance, active-effect crash recovery, SCM retry-policy, signed-helper, control-streaming, admission, activation, signing, notarization, or production-readiness proof."
        Receipt = "restart-recovery-live-proof.json"; Digest = "restart-recovery-live-proof.sha256"; MaxBytes = 4096
        Keys = @("schema","category","origin","source_head_commit","source_tree_id","cargo_executable_sha256","windows_cargo_executable_sha256","windows_rustc_executable_sha256","windows_msvc_environment_sha256","service_executable_sha256","rebuilt_service_executable_sha256","frozen_database_sha256","continuity_sha256","activation_evidence_sha256","migration_backups_sha256","controller_definition_sha256","harness_definition_sha256","windows_control_definition_sha256","proof_transcript_sha256","proof_identity","observed_at_ms","status","proof_boundary")
    }
    mac_windows_control_event_streaming_live = [ordered]@{
        Origin = "mac_windows_control_event_streaming_proof_controller"; Schema = "assemblywright.mac-windows-control-streaming-live-proof.v1"
        ProofIdentity = "assemblywright.mac-windows-control-event-streaming-live.v1"
        Boundary = "Exact clean published main used the committed native Mac/Windows bridge harness through fixed Bash stdin in --run-relay mode. One independently signed Swift helper and the exact signed Rust agent completed exporter-bound mTLS owner-control projection plus durable same-stream advancing event-cursor recovery after a fresh helper and agent restart. The private coordination transcript was hashed and deleted. This is proof production only: it does not admit evidence, approve or activate orchestration, grant protocol/schema/runtime authority, prove current-source linkage of the built binaries, Developer ID distribution, notarization, clean-profile installation, unattended operation, or production readiness."
        Receipt = "mac-windows-control-streaming-live-proof.json"; Digest = "mac-windows-control-streaming-live-proof.sha256"; MaxBytes = 4096
        Keys = @("schema","category","origin","head_commit","tree_id","controller_definition_sha256","mac_live_harness_definition_sha256","signed_helper_executable_sha256","signed_helper_cdhash","signed_helper_team","signed_helper_identifier","signed_agent_executable_sha256","signed_agent_cdhash","signed_agent_team","signed_agent_identifier","event_stream_transcript_sha256","proof_identity","observed_at_ms","status","proof_boundary")
    }
}

if ($null -eq ("Assemblywright.HeldEvidenceFile" -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Assemblywright {
  [StructLayout(LayoutKind.Sequential)]
  internal struct ByHandleFileInformation {
    internal uint FileAttributes;
    internal System.Runtime.InteropServices.ComTypes.FILETIME CreationTime;
    internal System.Runtime.InteropServices.ComTypes.FILETIME LastAccessTime;
    internal System.Runtime.InteropServices.ComTypes.FILETIME LastWriteTime;
    internal uint VolumeSerialNumber;
    internal uint FileSizeHigh;
    internal uint FileSizeLow;
    internal uint NumberOfLinks;
    internal uint FileIndexHigh;
    internal uint FileIndexLow;
  }

  public sealed class HeldEvidenceFile : IDisposable {
    const uint GenericRead = 0x80000000;
    const uint FileShareRead = 0x00000001;
    const uint OpenExisting = 3;
    const uint FileFlagOpenReparsePoint = 0x00200000;
    const uint FileAttributeDirectory = 0x00000010;
    const uint FileAttributeReparsePoint = 0x00000400;
    const uint FileNameNormalized = 0;
    const uint VolumeNameDos = 0;

    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFileW(string name, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle handle, out ByHandleFileInformation information);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern uint GetFinalPathNameByHandleW(SafeFileHandle handle, char[] path, uint count, uint flags);

    SafeFileHandle handle;
    FileStream stream;
    readonly string openedIdentity;
    readonly string finalPath;

    HeldEvidenceFile(SafeFileHandle opened) {
      handle = opened;
      openedIdentity = Identity(opened);
      finalPath = FinalPath(opened);
      stream = new FileStream(opened, FileAccess.Read, 4096, false);
    }

    public static HeldEvidenceFile Open(string path) {
      string expected = Path.GetFullPath(path);
      SafeFileHandle opened = CreateFileW(expected, GenericRead, FileShareRead, IntPtr.Zero, OpenExisting, FileFlagOpenReparsePoint, IntPtr.Zero);
      if (opened.IsInvalid) { int error=Marshal.GetLastWin32Error(); opened.Dispose(); throw new Win32Exception(error, "Evidence handle open failed."); }
      try {
        ByHandleFileInformation info;
        if (!GetFileInformationByHandle(opened, out info)) throw new Win32Exception(Marshal.GetLastWin32Error(), "Evidence identity read failed.");
        if ((info.FileAttributes & (FileAttributeDirectory | FileAttributeReparsePoint)) != 0 || info.NumberOfLinks != 1)
          throw new InvalidDataException("Evidence handle is not an ordinary single-link file.");
        string finalPath = FinalPath(opened);
        if (!String.Equals(expected, finalPath, StringComparison.OrdinalIgnoreCase))
          throw new InvalidDataException("Evidence handle canonical path is ambiguous.");
        return new HeldEvidenceFile(opened);
      } catch { opened.Dispose(); throw; }
    }

    public byte[] ReadAll(long maximum) {
      ValidateHeld();
      if (stream.Length <= 0 || stream.Length > maximum || stream.Length > Int32.MaxValue)
        throw new InvalidDataException("Evidence file size is outside the fixed bound.");
      byte[] bytes = new byte[(int)stream.Length];
      stream.Position = 0;
      int offset=0;
      while (offset < bytes.Length) { int count=stream.Read(bytes, offset, bytes.Length-offset); if (count==0) throw new EndOfStreamException(); offset += count; }
      ValidateHeld();
      return bytes;
    }

    public void RevalidatePath(string path) {
      ValidateHeld();
      using (HeldEvidenceFile current=Open(path)) {
        if (!String.Equals(openedIdentity, current.openedIdentity, StringComparison.Ordinal) ||
            !String.Equals(finalPath, current.finalPath, StringComparison.OrdinalIgnoreCase))
          throw new InvalidDataException("Evidence path no longer names the held file.");
      }
      ValidateHeld();
    }

    void ValidateHeld() {
      if (handle==null || handle.IsClosed || handle.IsInvalid || !String.Equals(openedIdentity, Identity(handle), StringComparison.Ordinal) ||
          !String.Equals(finalPath, FinalPath(handle), StringComparison.OrdinalIgnoreCase))
        throw new InvalidDataException("Held evidence identity drifted.");
    }

    static string Identity(SafeFileHandle value) {
      ByHandleFileInformation info;
      if (!GetFileInformationByHandle(value, out info)) throw new Win32Exception(Marshal.GetLastWin32Error(), "Evidence identity read failed.");
      if ((info.FileAttributes & (FileAttributeDirectory | FileAttributeReparsePoint)) != 0 || info.NumberOfLinks != 1)
        throw new InvalidDataException("Held evidence is not an ordinary single-link file.");
      return String.Format("{0:x8}:{1:x8}:{2:x8}:{3:x8}:{4:x8}:{5:x8}:{6:x8}:{7:x8}", info.VolumeSerialNumber,
        info.FileIndexHigh, info.FileIndexLow, info.FileSizeHigh, info.FileSizeLow, info.LastWriteTime.dwHighDateTime,
        info.LastWriteTime.dwLowDateTime, info.FileAttributes);
    }

    static string FinalPath(SafeFileHandle value) {
      char[] buffer=new char[32768];
      uint length=GetFinalPathNameByHandleW(value, buffer, (uint)buffer.Length, FileNameNormalized | VolumeNameDos);
      if (length==0 || length>=buffer.Length) throw new Win32Exception(Marshal.GetLastWin32Error(), "Evidence final path read failed.");
      string path=new string(buffer, 0, (int)length);
      const string prefix=@"\\?\";
      if (!path.StartsWith(prefix, StringComparison.Ordinal) || path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
        throw new InvalidDataException("Evidence final path namespace is unsupported.");
      return path.Substring(prefix.Length);
    }

    public void Dispose() {
      if (stream!=null) { stream.Dispose(); stream=null; handle=null; }
      else if (handle!=null) { handle.Dispose(); handle=null; }
    }
  }
}
'@
}

function Assert-ExactKeys {
    param($Value, [string[]]$Keys, [string]$Label)
    if ($Value -is [System.Collections.IDictionary]) {
        $actual = @($Value.Keys | ForEach-Object { [string]$_ } | Sort-Object -CaseSensitive)
    } else {
        $actual = @($Value.PSObject.Properties.Name | Sort-Object -CaseSensitive)
    }
    $expected = @($Keys | Sort-Object -CaseSensitive)
    if ($actual.Count -ne $expected.Count) { throw "$Label has an unexpected JSON shape." }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        if ($actual[$index] -cne $expected[$index]) { throw "$Label has an unexpected JSON shape." }
    }
}

function Assert-NoReparseComponents {
    param([Parameter(Mandatory = $true)][string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    if ($full.StartsWith("\\?\", [StringComparison]::Ordinal) -or $full -notmatch '^[A-Za-z]:\\') {
        throw "An evidence path used an unsupported namespace."
    }
    $root = [IO.Path]::GetPathRoot($full)
    $current = $root
    foreach ($part in @($full.Substring($root.Length) -split "[\\/]" | Where-Object { $_.Length -gt 0 })) {
        $current = Join-Path $current $part
        if (-not (Test-Path -LiteralPath $current)) { throw "An evidence path component is missing." }
        if (((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "An evidence path component is a reparse point."
        }
    }
    return $full
}

function Assert-OwnerSystemFile {
    param([Parameter(Mandatory = $true)][string]$Path, [switch]$AllowInherited)
    $full = Assert-NoReparseComponents $Path
    $item = Get-Item -LiteralPath $full -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Evidence must be an ordinary file."
    }
    $links = @(& "$env:SystemRoot\System32\fsutil.exe" hardlink list $full 2>$null | Where-Object { $_.Trim().Length -gt 0 })
    if ($LASTEXITCODE -ne 0 -or $links.Count -ne 1) { throw "Evidence must be single-link." }
    $acl = Get-Acl -LiteralPath $full
    $ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $systemSid = New-Object Security.Principal.SecurityIdentifier([Security.Principal.WellKnownSidType]::LocalSystemSid, $null)
    $actualOwnerSid = (New-Object Security.Principal.NTAccount($acl.Owner)).Translate([Security.Principal.SecurityIdentifier])
    if ((-not $AllowInherited -and -not $acl.AreAccessRulesProtected) -or $actualOwnerSid.Value -cne $ownerSid.Value) {
        throw "Evidence must have the exact protected owner."
    }
    $seen = @{}
    foreach ($rule in @($acl.Access)) {
        $sid = $rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value
        if ((-not $AllowInherited -and $rule.IsInherited) -or $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
            @($ownerSid.Value, $systemSid.Value) -cnotcontains $sid -or
            (($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -ne [Security.AccessControl.FileSystemRights]::FullControl)) {
            throw "Evidence ACL must be limited to owner and SYSTEM full control."
        }
        if ($seen.ContainsKey($sid)) { throw "Evidence ACL contains duplicate principals." }
        $seen[$sid] = $true
    }
    if ($seen.Count -ne 2 -or -not $seen.ContainsKey($ownerSid.Value) -or -not $seen.ContainsKey($systemSid.Value)) {
        throw "Evidence ACL principal set is incomplete."
    }
    return $full
}

function Read-BoundedFileBytes {
    param([string]$Path, [UInt64]$Maximum, [switch]$SkipIdentity, [switch]$AllowInherited)
    $full = if ($SkipIdentity) { [IO.Path]::GetFullPath($Path) } else { Assert-OwnerSystemFile $Path -AllowInherited:$AllowInherited }
    $stream = New-Object IO.FileStream($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($stream.Length -eq 0 -or [UInt64]$stream.Length -gt $Maximum) { throw "Evidence file size is outside the fixed bound." }
        $bytes = New-Object byte[] ([int]$stream.Length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -eq 0) { throw "Evidence file ended before its measured length." }
            $offset += $read
        }
        return ,$bytes
    } finally {
        $stream.Dispose()
    }
}

function Get-Sha256Hex {
    param([byte[]]$Bytes)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant() }
    finally { $sha.Dispose() }
}

function Get-UtcMilliseconds {
    return [UInt64](([DateTime]::UtcNow.Ticks - 621355968000000000) / [TimeSpan]::TicksPerMillisecond)
}

function Assert-ReceiptSemantics {
    param($Value, [string]$Category, $Contract)
    $commitFields = switch ($Category) {
        "github_publication_live" { @("source_head_commit", "source_tree_id", "published_main_commit") }
        "restart_recovery_live" { @("source_head_commit", "source_tree_id") }
        default { @("head_commit", "tree_id") }
    }
    foreach ($field in $commitFields) {
        if ($Value.$field -isnot [string] -or [string]$Value.$field -cnotmatch '^[0-9a-f]{40}$' -or
            [string]$Value.$field -cmatch '^0{40}$') {
            throw "A receipt commit or tree identity is malformed."
        }
    }
    foreach ($property in @($Value.PSObject.Properties | Where-Object { $_.Name -like "*_sha256" })) {
        if ($property.Value -isnot [string] -or [string]$property.Value -cnotmatch $shaPattern -or
            [string]$property.Value -cmatch '^0{64}$') {
            throw "A receipt SHA-256 field is malformed or zero."
        }
    }
    if ($Value.proof_boundary -isnot [string] -or $Value.proof_boundary.Length -lt 1 -or
        $Value.proof_boundary.Length -gt 2048 -or $Value.proof_boundary -match '[\x00-\x1f]') {
        throw "The receipt proof boundary is invalid."
    }
    if ([string]$Value.proof_boundary -cne [string]($Contract["Boundary"])) {
        throw "The receipt proof boundary is not the fixed controller boundary."
    }
    if ($Contract.Contains("GateIdentity") -and [string]$Value.gate_identity -cne [string]($Contract["GateIdentity"])) {
        throw "The receipt gate identity is not fixed."
    }
    if ($Contract.Contains("ProofIdentity") -and [string]$Value.proof_identity -cne [string]($Contract["ProofIdentity"])) {
        throw "The receipt proof identity is not fixed."
    }
    if ($Category -eq "review_provider_live" -and
        ([string]$Value.provider_id -cne "openai.codex" -or [string]$Value.model_id -cne "gpt-5.6-sol")) {
        throw "The review-provider receipt identity is not fixed."
    }
    if ($Category -eq "mac_windows_control_event_streaming_live") {
        if ([string]$Value.signed_helper_team -cne "H686S3N4V9" -or
            [string]$Value.signed_helper_identifier -cne "com.nobiletechnology.assemblywright.developer-bridge.cli" -or
            [string]$Value.signed_helper_cdhash -cnotmatch '^[0-9a-f]{40,64}$' -or
            [string]$Value.signed_helper_cdhash -cmatch '^0+$' -or
            [string]$Value.signed_agent_cdhash -cnotmatch '^[0-9a-f]{40,64}$' -or
            [string]$Value.signed_agent_cdhash -cmatch '^0+$' -or
            ([string]$Value.signed_agent_team -cne "not set" -and [string]$Value.signed_agent_team -cnotmatch '^[A-Z0-9]{10}$') -or
            [string]$Value.signed_agent_identifier -cnotmatch '^[A-Za-z0-9._-]{1,128}$') {
            throw "The signed helper or agent identity is malformed."
        }
    }
}

function ConvertTo-CanonicalReceiptLine {
    param($Value, $Contract)
    $parts = @()
    foreach ($key in @($Contract["Keys"])) {
        $property = $Value.PSObject.Properties[$key]
        if ($null -eq $property) { throw "The receipt cannot be reconstructed canonically." }
        if ($key -ceq "observed_at_ms") {
            if ($property.Value -isnot [Int64] -or [Int64]$property.Value -le 0) {
                throw "The canonical receipt time is invalid."
            }
            $parts += '"observed_at_ms":' + ([Int64]$property.Value).ToString([Globalization.CultureInfo]::InvariantCulture)
        } else {
            if ($property.Value -isnot [string] -or [string]$property.Value -match '["\\\x00-\x1f]') {
                throw "A canonical receipt string requires unsupported JSON escaping."
            }
            $parts += '"' + $key + '":"' + [string]$property.Value + '"'
        }
    }
    return '{' + ($parts -join ',') + '}' + "`n"
}

function Read-ProofPair {
    param([string]$Receipt, [string]$Digest, [switch]$SkipIdentity)
    if ([string]::IsNullOrWhiteSpace($Receipt) -or [string]::IsNullOrWhiteSpace($Digest)) {
        throw "Both -ReceiptPath and -DigestPath are required for this action."
    }
    $receiptFull = if ($SkipIdentity) { [IO.Path]::GetFullPath($Receipt) } else { Assert-OwnerSystemFile $Receipt }
    $digestFull = if ($SkipIdentity) { [IO.Path]::GetFullPath($Digest) } else { Assert-OwnerSystemFile $Digest }
    $receiptHandle = [Assemblywright.HeldEvidenceFile]::Open($receiptFull)
    $digestHandle = $null
    try {
        $digestHandle = [Assemblywright.HeldEvidenceFile]::Open($digestFull)
        $receiptHandle.RevalidatePath($receiptFull)
        $digestHandle.RevalidatePath($digestFull)
        if (-not $SkipIdentity) {
            [void](Assert-OwnerSystemFile $receiptFull)
            [void](Assert-OwnerSystemFile $digestFull)
        }
        $digestBytes = $digestHandle.ReadAll($maxDigestBytes)
        $receiptBytes = $receiptHandle.ReadAll(4096)
        $digestText = [Text.Encoding]::ASCII.GetString($digestBytes)
        if ($digestBytes.Length -ne 65 -or $digestText.Substring(64, 1) -cne "`n" -or
            $digestText.Substring(0, 64) -cnotmatch $shaPattern) {
            throw "The raw SHA-256 sidecar has the wrong exact shape."
        }
        $expectedDigest = $digestText.Substring(0, 64)
        $actualDigest = Get-Sha256Hex $receiptBytes
        if ($actualDigest -cne $expectedDigest) { throw "The raw receipt SHA-256 does not match its sidecar." }
        $utf8 = New-Object Text.UTF8Encoding($false, $true)
        $receiptText = $utf8.GetString($receiptBytes)
        if (-not $receiptText.EndsWith("`n", [StringComparison]::Ordinal) -or $receiptText.Contains("`r") -or
            $receiptText.Substring(0, $receiptText.Length - 1).Contains("`n")) {
            throw "The receipt must be one canonical UTF-8 JSON line."
        }
        try { $value = $receiptText | ConvertFrom-Json }
        catch { throw "The receipt JSON is malformed." }
        $category = [string]$value.category
        if (-not $contracts.ContainsKey($category)) { throw "The receipt category is unsupported." }
        $contract = $contracts[$category]
        Assert-ExactKeys $value $contract["Keys"] "Proof-controller receipt"
        if ([IO.Path]::GetFileName($receiptFull) -cne $contract["Receipt"] -or
            [IO.Path]::GetFileName($digestFull) -cne $contract["Digest"]) {
            throw "The receipt and sidecar names do not form the fixed category pair."
        }
        if ($receiptBytes.Length -gt [int]($contract["MaxBytes"]) -or [string]$value.schema -cne $contract["Schema"] -or
            [string]$value.origin -cne $contract["Origin"] -or [string]$value.status -cne "passed") {
            throw "The receipt schema, category binding, status, or size is invalid."
        }
        if ($value.observed_at_ms -isnot [Int64] -or [Int64]$value.observed_at_ms -le 0 -or
            [UInt64]$value.observed_at_ms -gt (Get-UtcMilliseconds)) {
            throw "The receipt observed time is invalid."
        }
        Assert-ReceiptSemantics $value $category $contract
        if ((ConvertTo-CanonicalReceiptLine $value $contract) -cne $receiptText) {
            throw "The receipt bytes are not the exact canonical controller encoding."
        }
        $receiptHandle.RevalidatePath($receiptFull)
        $digestHandle.RevalidatePath($digestFull)
        if (-not $SkipIdentity) {
            [void](Assert-OwnerSystemFile $receiptFull)
            [void](Assert-OwnerSystemFile $digestFull)
        }
        return [ordered]@{ Category = $category; Origin = [string]($contract["Origin"]); Digest = $actualDigest; ObservedAtMs = [UInt64]$value.observed_at_ms }
    } finally {
        if ($null -ne $digestHandle) { $digestHandle.Dispose() }
        if ($null -ne $receiptHandle) { $receiptHandle.Dispose() }
    }
}

function Read-OwnerToken {
    $tokenPath = Join-Path $DataDir "development.token"
    $bytes = Read-BoundedFileBytes $tokenPath 257 -AllowInherited
    if (@($bytes | Where-Object { [int]$_ -gt 127 }).Count -ne 0) {
        throw "The Windows-local owner token is invalid."
    }
    $rawToken = [Text.Encoding]::ASCII.GetString($bytes)
    $token = if ($rawToken.EndsWith("`n", [StringComparison]::Ordinal)) {
        $rawToken.Substring(0, $rawToken.Length - 1)
    } else {
        $rawToken
    }
    if ($token.Length -lt 32 -or $token.Length -gt 256 -or $token -notmatch '^[\x21-\x7e]+$') {
        throw "The Windows-local owner token is invalid."
    }
    return $token
}

function Invoke-AdmissionPreflight {
    $token = Read-OwnerToken
    $headers = @{ Authorization = "Bearer $token" }
    try {
        try { $projection = Invoke-RestMethod -Method Get -Uri "http://$endpoint/v1/feature-conveyor/activation-evidence" -Headers $headers }
        catch { throw "The owner-authenticated loopback evidence preflight failed." }
    } finally {
        $headers = $null
        $token = $null
    }
    Assert-ExactKeys $projection @("schema_version","emergency_paused","emergency_pause_revision","activation_status","activation_id","evidence") "Evidence admission preflight"
    Assert-ExactKeys $projection.evidence $evidenceKeys "Evidence admission references"
    if ([UInt64]$projection.schema_version -ne $schemaVersion -or
        @("inactive", "active") -cnotcontains [string]$projection.activation_status -or
        $projection.emergency_paused -isnot [bool] -or
        (([string]$projection.activation_status -eq "active") -ne ($null -ne $projection.activation_id))) {
        throw "The evidence admission preflight contract is invalid."
    }
    foreach ($key in $evidenceKeys) {
        $reference = $projection.evidence.$key
        if ($null -ne $reference) {
            Assert-ExactKeys $reference @("evidence_id","revision","receipt_sha256") "Evidence admission reference"
            try { $referenceId = [guid]([string]$reference.evidence_id) } catch { throw "An evidence admission reference is malformed." }
            $digestValues = @($reference.receipt_sha256)
            if ($referenceId -eq [guid]::Empty -or [UInt64]$reference.revision -eq 0 -or
                $digestValues.Count -ne 32 -or @($digestValues | Where-Object { [Int64]$_ -lt 0 -or [Int64]$_ -gt 255 }).Count -ne 0 -or
                @($digestValues | Where-Object { [Int64]$_ -ne 0 }).Count -eq 0) {
                throw "An evidence admission reference is malformed."
            }
        }
    }
    return $projection
}

function Convert-DigestToByteArray {
    param([string]$Digest)
    $values = @()
    for ($index = 0; $index -lt 64; $index += 2) { $values += [Convert]::ToByte($Digest.Substring($index, 2), 16) }
    return ,$values
}

function Convert-ReferenceDigestToHex {
    param($Reference)
    if ($null -eq $Reference) { return $null }
    $builder = New-Object Text.StringBuilder
    foreach ($byte in @($Reference.receipt_sha256)) { [void]$builder.Append(([byte]$byte).ToString("x2")) }
    return $builder.ToString()
}

function Resolve-AdmissionPreflight {
    param($Projection, $Pair)
    $current = $Projection.evidence.PSObject.Properties[$Pair.Category].Value
    $currentDigest = Convert-ReferenceDigestToHex $current
    if ($currentDigest -ceq $Pair.Digest) {
        return [ordered]@{ AlreadyAdmitted = $true; Current = $current }
    }
    if ([string]$Projection.activation_status -ne "inactive") {
        throw "Activation evidence is immutable after activation."
    }
    if ([bool]$Projection.emergency_paused) {
        throw "Evidence admission is blocked by Emergency Pause."
    }
    return [ordered]@{ AlreadyAdmitted = $false; Current = $current }
}

function Write-RedactedStatus {
    param($Projection, [string]$Status = "evidence_admission_status")
    $entries = @()
    foreach ($key in $evidenceKeys) {
        $reference = $Projection.evidence.$key
        $entries += [ordered]@{
            category = $key
            admitted = ($null -ne $reference)
            revision = if ($null -eq $reference) { [UInt64]0 } else { [UInt64]$reference.revision }
            receipt_sha256 = Convert-ReferenceDigestToHex $reference
        }
    }
    [ordered]@{
        schema_version = $schemaVersion
        status = $Status
        activation_status = [string]$Projection.activation_status
        emergency_paused = [bool]$Projection.emergency_paused
        emergency_pause_revision = [UInt64]$Projection.emergency_pause_revision
        evidence = $entries
    } | ConvertTo-Json -Compress -Depth 5
}

function Invoke-SelfTest {
    $root = Join-Path ([IO.Path]::GetTempPath()) ("assemblywright-evidence-admission-" + [guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $root | Out-Null
    try {
        $receipt = Join-Path $root "repository-gate-proof.json"
        $digest = Join-Path $root "repository-gate-proof.sha256"
        $boundary = [string]($contracts["repository_gate_proof"]["Boundary"])
        $json = '{"schema":"assemblywright.repository-gate-proof.v1","category":"repository_gate_proof","origin":"repository_gate_proof_controller","head_commit":"1111111111111111111111111111111111111111","tree_id":"2222222222222222222222222222222222222222","release_local_definition_sha256":"3333333333333333333333333333333333333333333333333333333333333333","gate_identity":"assemblywright.release-local.v1","observed_at_ms":1700000000000,"status":"passed","proof_boundary":"' + $boundary + '"}' + "`n"
        [IO.File]::WriteAllText($receipt, $json, (New-Object Text.UTF8Encoding($false)))
        $validDigest = Get-Sha256Hex ([IO.File]::ReadAllBytes($receipt))
        [IO.File]::WriteAllText($digest, $validDigest + "`n", [Text.Encoding]::ASCII)
        [void](Read-ProofPair $receipt $digest -SkipIdentity)
        $heldSharing = [Assemblywright.HeldEvidenceFile]::Open($receipt)
        $writer = $null
        try {
            $writeDenied = $false
            try { $writer = [IO.File]::Open($receipt, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite) }
            catch { $writeDenied = $true }
            if ($null -ne $writer) { $writer.Dispose(); $writer = $null }
            $deleteDenied = $false
            try { [IO.File]::Delete($receipt) } catch { $deleteDenied = $true }
            if (-not $writeDenied -or -not $deleteDenied) { throw "Held evidence sharing allowed write or delete." }
            $heldSharing.RevalidatePath($receipt)
        } finally {
            if ($null -ne $writer) { $writer.Dispose() }
            $heldSharing.Dispose()
        }
        $retryReference = [pscustomobject]@{
            evidence_id = [guid]::NewGuid().ToString(); revision = [Int64]1
            receipt_sha256 = (Convert-DigestToByteArray $validDigest)
        }
        $retryProjection = [pscustomobject]@{
            activation_status = "active"; emergency_paused = $true
            evidence = [pscustomobject]@{ repository_gate_proof = $retryReference }
        }
        $retryPair = [ordered]@{ Category = "repository_gate_proof"; Digest = $validDigest }
        $retryDecision = Resolve-AdmissionPreflight $retryProjection $retryPair
        if (-not $retryDecision.AlreadyAdmitted) { throw "Exact-digest reconciliation did not precede pause and activation denial." }
        $retryPair.Digest = "4" * 64
        try { [void](Resolve-AdmissionPreflight $retryProjection $retryPair); throw "New evidence bypassed pause or activation in self-test." }
        catch { if ($_.Exception.Message -eq "New evidence bypassed pause or activation in self-test.") { throw } }

        [IO.File]::WriteAllText($digest, ("0" * 64) + "`n", [Text.Encoding]::ASCII)
        try { [void](Read-ProofPair $receipt $digest -SkipIdentity); throw "Wrong-digest self-test was accepted." } catch { if ($_.Exception.Message -eq "Wrong-digest self-test was accepted.") { throw } }
        [IO.File]::WriteAllText($digest, $validDigest + "`n", [Text.Encoding]::ASCII)
        $wrongPair = Join-Path $root "review-provider-live-proof.sha256"
        [IO.File]::Copy($digest, $wrongPair)
        try { [void](Read-ProofPair $receipt $wrongPair -SkipIdentity); throw "Wrong-pair self-test was accepted." } catch { if ($_.Exception.Message -eq "Wrong-pair self-test was accepted.") { throw } }

        foreach ($semanticCase in @(
            @("status", $json.Replace('"status":"passed"', '"status":"failed"')),
            @("schema", $json.Replace('assemblywright.repository-gate-proof.v1', 'assemblywright.repository-gate-proof.v2')),
            @("fixed_identity", $json.Replace('assemblywright.release-local.v1', 'assemblywright.release-local.v0')),
            @("duplicate_key", $json.Replace('"status":"passed",', '"status":"passed","status":"passed",')),
            @("field_order", $json.Replace('{"schema":"assemblywright.repository-gate-proof.v1","category":"repository_gate_proof"', '{"category":"repository_gate_proof","schema":"assemblywright.repository-gate-proof.v1"')),
            @("whitespace", $json.Replace(',"category":"repository_gate_proof"', ', "category":"repository_gate_proof"')),
            @("rewritten_boundary", $json.Replace($boundary, "rewritten boundary"))
        )) {
            [IO.File]::WriteAllText($receipt, [string]($semanticCase[1]), (New-Object Text.UTF8Encoding($false)))
            $semanticDigest = Get-Sha256Hex ([IO.File]::ReadAllBytes($receipt))
            [IO.File]::WriteAllText($digest, $semanticDigest + "`n", [Text.Encoding]::ASCII)
            try { [void](Read-ProofPair $receipt $digest -SkipIdentity); throw "Semantic $($semanticCase[0]) self-test was accepted." }
            catch { if ($_.Exception.Message -eq "Semantic $($semanticCase[0]) self-test was accepted.") { throw } }
        }
        [IO.File]::WriteAllText($receipt, "{malformed}`n", (New-Object Text.UTF8Encoding($false)))
        $malformedDigest = Get-Sha256Hex ([IO.File]::ReadAllBytes($receipt))
        [IO.File]::WriteAllText($digest, $malformedDigest + "`n", [Text.Encoding]::ASCII)
        try { [void](Read-ProofPair $receipt $digest -SkipIdentity); throw "Malformed self-test was accepted." } catch { if ($_.Exception.Message -eq "Malformed self-test was accepted.") { throw } }
        [IO.File]::WriteAllBytes($receipt, (New-Object byte[] 4097))
        $oversizeDigest = Get-Sha256Hex ([IO.File]::ReadAllBytes($receipt))
        [IO.File]::WriteAllText($digest, $oversizeDigest + "`n", [Text.Encoding]::ASCII)
        try { [void](Read-ProofPair $receipt $digest -SkipIdentity); throw "Oversize self-test was accepted." } catch { if ($_.Exception.Message -eq "Oversize self-test was accepted.") { throw } }
        '{"schema_version":1,"status":"owner_evidence_admission_self_test_passed","success":"verified","held_identity":"verified","write_delete_sharing":"denied","exact_digest_reconciliation":"verified","wrong_digest":"rejected","wrong_pair":"rejected","wrong_status":"rejected","wrong_schema":"rejected","wrong_fixed_identity":"rejected","duplicate_key":"rejected","field_order":"rejected","whitespace":"rejected","rewritten_boundary":"rejected","malformed":"rejected","oversize":"rejected"}'
    } finally {
        if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
    }
}

if ($Action -eq "SelfTest") {
    if ($Confirm -or $ReceiptPath.Length -ne 0 -or $DigestPath.Length -ne 0) { throw "SelfTest accepts no receipt, digest, or confirmation." }
    Invoke-SelfTest
    exit 0
}

if ($Action -ne "Admit" -and $Confirm) { throw "-Confirm is valid only with -Action Admit." }
if ($Action -eq "Admit" -and -not $Confirm) { throw "Evidence admission requires explicit -Confirm." }
if ($Action -eq "Status" -and ($ReceiptPath.Length -ne 0 -or $DigestPath.Length -ne 0)) {
    throw "Status accepts no evidence paths."
}

$pair = $null
if ($Action -in @("Check", "Admit") -and ($ReceiptPath.Length -ne 0 -or $DigestPath.Length -ne 0)) {
    $pair = Read-ProofPair $ReceiptPath $DigestPath
} elseif ($Action -eq "Admit") {
    throw "Admit requires one exact receipt and sidecar pair."
}
$projection = Invoke-AdmissionPreflight

if ($Action -eq "Status" -or ($Action -eq "Check" -and $null -eq $pair)) {
    Write-RedactedStatus $projection $(if ($Action -eq "Check") { "evidence_admission_check_passed" } else { "evidence_admission_status" })
    exit 0
}
if ($Action -eq "Check") {
    [ordered]@{
        schema_version = $schemaVersion; status = "evidence_pair_check_passed"; category = $pair.Category
        origin = $pair.Origin; receipt_sha256 = $pair.Digest; observed_at_ms = $pair.ObservedAtMs
        emergency_paused = [bool]$projection.emergency_paused; emergency_pause_revision = [UInt64]$projection.emergency_pause_revision
    } | ConvertTo-Json -Compress
    exit 0
}

$decision = Resolve-AdmissionPreflight $projection $pair
$current = $decision.Current
if ($decision.AlreadyAdmitted) {
    [ordered]@{
        schema_version = $schemaVersion; status = "evidence_already_admitted"; category = $pair.Category
        revision = [UInt64]$current.revision; receipt_sha256 = $pair.Digest
        emergency_pause_revision = [UInt64]$projection.emergency_pause_revision
    } | ConvertTo-Json -Compress
    exit 0
}
$currentRevision = if ($null -eq $current) { [UInt64]0 } else { [UInt64]$current.revision }
$submittedEvidenceId = [guid]::NewGuid()
$body = [ordered]@{
    schema_version = $schemaVersion
    category = $pair.Category
    origin = $pair.Origin
    evidence_id = $submittedEvidenceId.ToString().ToLowerInvariant()
    revision = [UInt64]($currentRevision + 1)
    expected_current_revision = $currentRevision
    receipt_sha256 = (Convert-DigestToByteArray $pair.Digest)
    observed_at_ms = $pair.ObservedAtMs
    expected_emergency_pause_revision = [UInt64]$projection.emergency_pause_revision
} | ConvertTo-Json -Compress
$token = Read-OwnerToken
$headers = @{ Authorization = "Bearer $token" }
try {
    try { $admitted = Invoke-RestMethod -Method Post -Uri "http://$endpoint/v1/feature-conveyor/activation-evidence" -Headers $headers -ContentType "application/json" -Body $body }
    catch { throw "Evidence admission was rejected; rerun Status or Check before a deliberate retry." }
} finally {
    $headers = $null
    $token = $null
    $body = $null
}
Assert-ExactKeys $admitted @("schema_version","category","origin","evidence","observed_at_ms","emergency_pause_revision") "Evidence admission receipt"
Assert-ExactKeys $admitted.evidence @("evidence_id","revision","receipt_sha256") "Admitted evidence reference"
try { $admittedEvidenceId = [guid]([string]$admitted.evidence.evidence_id) }
catch { throw "The evidence admission receipt returned an invalid evidence ID." }
if ([UInt64]$admitted.schema_version -ne $schemaVersion -or [string]$admitted.category -cne $pair.Category -or
    [string]$admitted.origin -cne $pair.Origin -or [UInt64]$admitted.evidence.revision -ne ($currentRevision + 1) -or
    $admittedEvidenceId -eq [guid]::Empty -or $admittedEvidenceId -ne $submittedEvidenceId -or
    (Convert-ReferenceDigestToHex $admitted.evidence) -cne $pair.Digest -or
    [UInt64]$admitted.observed_at_ms -ne [UInt64]$pair.ObservedAtMs -or
    [UInt64]$admitted.emergency_pause_revision -ne [UInt64]$projection.emergency_pause_revision) {
    throw "The evidence admission receipt did not match the deliberate request."
}
[ordered]@{
    schema_version = $schemaVersion; status = "evidence_admitted"; category = $pair.Category
    revision = [UInt64]$admitted.evidence.revision; evidence_id = [string]$admitted.evidence.evidence_id
    receipt_sha256 = $pair.Digest; emergency_pause_revision = [UInt64]$admitted.emergency_pause_revision
} | ConvertTo-Json -Compress

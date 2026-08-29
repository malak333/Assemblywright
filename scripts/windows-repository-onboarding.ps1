param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Plan", "Check", "Approve", "SelfTest")]
    [string]$Action,

    [string]$RepositoryPath = "",
    [string]$PlanId = "",
    [string]$DataDir = "",
    [ValidatePattern("^127\.0\.0\.1:[0-9]{1,5}$")]
    [string]$Endpoint = "127.0.0.1:7791",
    [switch]$ConfirmRegistration,
    [switch]$ConfirmCloudDisclosure,
    [switch]$ConfirmAutonomousPublication
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ownerControlSchemaVersion = 1
$planSchemaVersion = 1
$planLifetimeMs = [UInt64](24 * 60 * 60 * 1000)
$maximumPlanBytes = 8192
$maximumTokenBytes = 257
$uuidPattern = "^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
$commitPattern = "^[0-9a-f]{40}$"
$shaPattern = "^[0-9a-f]{64}$"
$planStatus = "repository_onboarding_plan"
$receiptStatus = "repository_onboarding_ready"
$baseBranch = "main"
$planDirectoryLeaf = "repository-onboarding-plans"

if ($DataDir.Length -eq 0) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw "The Windows-local master data directory is unavailable."
    }
    $DataDir = Join-Path $env:LOCALAPPDATA "Assemblywright\master"
}

foreach ($gitEnvironmentEntry in @(Get-ChildItem Env: | Where-Object { $_.Name -like "GIT_*" })) {
    Remove-Item -LiteralPath "Env:$($gitEnvironmentEntry.Name)"
}
$env:GIT_CONFIG_GLOBAL = "NUL"
$env:GIT_CONFIG_SYSTEM = "NUL"
$env:GIT_CONFIG_NOSYSTEM = "1"
$env:GIT_ATTR_NOSYSTEM = "1"
$env:GIT_TERMINAL_PROMPT = "0"
$env:GIT_OPTIONAL_LOCKS = "0"

if ($null -eq ("Assemblywright.HeldOnboardingFile" -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Assemblywright {
  [StructLayout(LayoutKind.Sequential)]
  internal struct OnboardingFileInformation {
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

  public sealed class HeldOnboardingFile : IDisposable {
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
    static extern bool GetFileInformationByHandle(SafeFileHandle handle, out OnboardingFileInformation information);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern uint GetFinalPathNameByHandleW(SafeFileHandle handle, char[] path, uint count, uint flags);

    SafeFileHandle handle;
    FileStream stream;
    readonly string identity;
    readonly string finalPath;

    HeldOnboardingFile(SafeFileHandle opened) {
      handle=opened;
      OnboardingFileInformation information;
      if (!GetFileInformationByHandle(handle, out information)) throw new Win32Exception();
      if ((information.FileAttributes & (FileAttributeDirectory | FileAttributeReparsePoint)) != 0 || information.NumberOfLinks != 1) {
        throw new InvalidDataException("The onboarding file was not an ordinary single-link file.");
      }
      identity=information.VolumeSerialNumber.ToString("x8") + ":" + information.FileIndexHigh.ToString("x8") + information.FileIndexLow.ToString("x8");
      finalPath=FinalPath(handle);
      stream=new FileStream(handle, FileAccess.Read, 4096, false);
    }

    static string FinalPath(SafeFileHandle value) {
      char[] buffer=new char[32768];
      uint length=GetFinalPathNameByHandleW(value, buffer, (uint)buffer.Length, FileNameNormalized | VolumeNameDos);
      if (length == 0 || length >= buffer.Length) throw new Win32Exception();
      string result=new string(buffer, 0, (int)length);
      if (result.StartsWith(@"\\?\", StringComparison.Ordinal)) result=result.Substring(4);
      return Path.GetFullPath(result);
    }

    public static HeldOnboardingFile Open(string path) {
      SafeFileHandle opened=CreateFileW(path, GenericRead, FileShareRead, IntPtr.Zero, OpenExisting, FileFlagOpenReparsePoint, IntPtr.Zero);
      if (opened.IsInvalid) { int error=Marshal.GetLastWin32Error(); opened.Dispose(); throw new Win32Exception(error); }
      try { return new HeldOnboardingFile(opened); } catch { opened.Dispose(); throw; }
    }

    public byte[] ReadAll(int maximum) {
      if (stream.Length <= 0 || stream.Length > maximum) throw new InvalidDataException("The onboarding file size was invalid.");
      byte[] bytes=new byte[(int)stream.Length];
      stream.Position=0;
      int offset=0;
      while (offset < bytes.Length) { int read=stream.Read(bytes, offset, bytes.Length-offset); if (read == 0) throw new EndOfStreamException(); offset += read; }
      return bytes;
    }

    public void RevalidatePath(string expected) {
      string canonical=Path.GetFullPath(expected);
      if (!string.Equals(canonical, finalPath, StringComparison.OrdinalIgnoreCase)) throw new InvalidDataException("The onboarding file canonical path changed.");
      using (HeldOnboardingFile reopened=Open(canonical)) {
        if (!string.Equals(reopened.identity, identity, StringComparison.Ordinal) || !string.Equals(reopened.finalPath, finalPath, StringComparison.OrdinalIgnoreCase)) {
          throw new InvalidDataException("The onboarding file identity changed.");
        }
      }
    }

    public void Dispose() { if (stream != null) { stream.Dispose(); stream=null; handle=null; } else if (handle != null) { handle.Dispose(); handle=null; } }
  }
}
'@
}

function Get-UtcMilliseconds {
    return [UInt64]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
}

function Get-Sha256Bytes {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return $algorithm.ComputeHash($Bytes) }
    finally { $algorithm.Dispose() }
}

function Get-Utf8Sha256Hex {
    param([Parameter(Mandatory = $true)][string]$Text)
    return Convert-BytesToHex (Get-Sha256Bytes ([Text.Encoding]::UTF8.GetBytes($Text)))
}

function Convert-BytesToHex {
    param([Parameter(Mandatory = $true)]$Bytes)
    return -join @($Bytes | ForEach-Object { ([byte]$_).ToString("x2") })
}

function Convert-HexToBytes {
    param([Parameter(Mandatory = $true)][string]$Hex)
    if ($Hex -cnotmatch $shaPattern) { throw "A canonical SHA-256 binding was malformed." }
    $bytes = New-Object byte[] 32
    for ($index = 0; $index -lt 32; $index += 1) {
        $bytes[$index] = [Convert]::ToByte($Hex.Substring($index * 2, 2), 16)
    }
    return $bytes
}

function Assert-ExactKeys {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string[]]$Keys,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ($Value -is [System.Collections.IDictionary]) {
        $actual = @($Value.Keys | ForEach-Object { [string]$_ } | Sort-Object -CaseSensitive)
    } else {
        $actual = @($Value.PSObject.Properties.Name | Sort-Object -CaseSensitive)
    }
    $expected = @($Keys | Sort-Object -CaseSensitive)
    if ($actual.Count -ne $expected.Count) { throw "$Label had an unexpected JSON shape." }
    for ($index = 0; $index -lt $expected.Count; $index += 1) {
        if ($actual[$index] -cne $expected[$index]) { throw "$Label had an unexpected JSON shape." }
    }
}

function Assert-NoReparseComponents {
    param([Parameter(Mandatory = $true)][string]$Path, [bool]$AllowMissingLeaf = $false)
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    if ([string]::IsNullOrWhiteSpace($root) -or $full.StartsWith("\\", [StringComparison]::Ordinal)) {
        throw "A bounded local path was not a fixed-volume absolute path."
    }
    $rootItem = Get-Item -LiteralPath $root -Force
    if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "A bounded local path root was a reparse point."
    }
    $parts = @($full.Substring($root.Length) -split "[\\/]" | Where-Object { $_.Length -gt 0 })
    $current = $root
    for ($index = 0; $index -lt $parts.Count; $index += 1) {
        $current = Join-Path $current $parts[$index]
        if (-not (Test-Path -LiteralPath $current)) {
            if ($AllowMissingLeaf -and $index -eq ($parts.Count - 1)) { return }
            throw "A bounded local path component was missing."
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "A bounded local path component was a reparse point."
        }
        if ($index -lt ($parts.Count - 1) -and -not $item.PSIsContainer) {
            throw "A bounded local path component was not a directory."
        }
    }
}

function Assert-FixedLocalPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    if ($full.StartsWith("\\", [StringComparison]::Ordinal) -or $full.StartsWith("//", [StringComparison]::Ordinal)) {
        throw "The repository path was not a local fixed-volume path."
    }
    $root = [IO.Path]::GetPathRoot($full)
    $drive = New-Object IO.DriveInfo($root)
    if (-not $drive.IsReady -or $drive.DriveType -ne [IO.DriveType]::Fixed) {
        throw "The repository path was not on a ready fixed local volume."
    }
    Assert-NoReparseComponents $full $false
    $item = Get-Item -LiteralPath $full -Force
    if (-not $item.PSIsContainer) { throw "The repository path was not a directory." }
    return $full.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Invoke-BoundedGit {
    param([Parameter(Mandatory = $true)][string]$Repository, [Parameter(Mandatory = $true)][string[]]$Arguments)
    $prior = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = @(& git --no-replace-objects -c core.fsmonitor=false -c core.hooksPath=NUL -C $Repository @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $prior
    }
    if ($exitCode -ne 0) { throw "Git rejected the bounded repository observation." }
    return ($output -join "`n").Trim()
}

function Assert-NoUnsupportedRepositoryMetadata {
    param(
        [Parameter(Mandatory = $true)][string]$Repository,
        [Parameter(Mandatory = $true)][string]$GitDirectory
    )
    foreach ($forbiddenPath in @(
        (Join-Path $Repository ".gitmodules"),
        (Join-Path $GitDirectory "config.worktree"),
        (Join-Path $GitDirectory "commondir"),
        (Join-Path $GitDirectory "gitdir"),
        (Join-Path $GitDirectory "worktrees"),
        (Join-Path $GitDirectory "modules")
    )) {
        if (Test-Path -LiteralPath $forbiddenPath) {
            throw "Worktree, linked, or submodule Git metadata is not eligible for onboarding."
        }
    }
}

function Assert-RepositoryEligible {
    param([Parameter(Mandatory = $true)][string]$Path, [string]$ExpectedHead = "")
    $repository = Assert-FixedLocalPath $Path
    $gitDirectory = Join-Path $repository ".git"
    $objectsDirectory = Join-Path $gitDirectory "objects"
    $headPath = Join-Path $gitDirectory "HEAD"
    $mainRefPath = Join-Path $gitDirectory "refs\heads\main"
    foreach ($requiredDirectory in @($gitDirectory, $objectsDirectory)) {
        Assert-NoReparseComponents $requiredDirectory $false
        if (-not (Test-Path -LiteralPath $requiredDirectory -PathType Container)) {
            throw "The repository did not use a standard Git directory."
        }
    }
    foreach ($requiredFile in @($headPath, $mainRefPath)) {
        Assert-NoReparseComponents $requiredFile $false
        if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
            throw "The repository did not use an exact loose symbolic main reference."
        }
    }
    Assert-NoUnsupportedRepositoryMetadata $repository $gitDirectory
    $topLevel = [IO.Path]::GetFullPath((Invoke-BoundedGit $repository @("rev-parse", "--show-toplevel")))
    $symbolicHead = Invoke-BoundedGit $repository @("symbolic-ref", "--quiet", "HEAD")
    $head = Invoke-BoundedGit $repository @("rev-parse", "HEAD")
    $branch = Invoke-BoundedGit $repository @("branch", "--show-current")
    $status = Invoke-BoundedGit $repository @("status", "--porcelain=v1", "--untracked-files=all")
    $tracked = Invoke-BoundedGit $repository @("ls-files", "-v", "--")
    $staged = Invoke-BoundedGit $repository @("ls-files", "--stage", "--")
    if (
        -not $topLevel.Equals($repository, [StringComparison]::OrdinalIgnoreCase) -or
        $symbolicHead -cne "refs/heads/main" -or $branch -cne $baseBranch -or
        $head -cnotmatch $commitPattern -or $status.Length -ne 0 -or
        @($tracked -split "`r?`n" | Where-Object { $_.Length -gt 0 -and $_ -cnotmatch "^H " }).Count -ne 0 -or
        @($staged -split "`r?`n" | Where-Object { $_ -match "^160000 " }).Count -ne 0
    ) {
        throw "The repository was not an exact clean standard main checkout with normal tracked-index state."
    }
    if ($ExpectedHead.Length -ne 0 -and $head -cne $ExpectedHead) {
        throw "The repository HEAD no longer matched the approved plan."
    }
    $expectedHeadBytes = [Text.Encoding]::ASCII.GetBytes("ref: refs/heads/main`n")
    $expectedRefBytes = [Text.Encoding]::ASCII.GetBytes("$head`n")
    $actualHeadBytes = [IO.File]::ReadAllBytes($headPath)
    $actualRefBytes = [IO.File]::ReadAllBytes($mainRefPath)
    if (
        (Compare-Object $actualHeadBytes $expectedHeadBytes -SyncWindow 0) -or
        (Compare-Object $actualRefBytes $expectedRefBytes -SyncWindow 0)
    ) {
        throw "The repository symbolic HEAD or exact loose main reference bytes drifted."
    }
    Assert-NoReparseComponents $repository $false
    Assert-NoReparseComponents $gitDirectory $false
    Assert-NoReparseComponents $objectsDirectory $false
    Assert-NoReparseComponents $headPath $false
    Assert-NoReparseComponents $mainRefPath $false
    return [ordered]@{ RepositoryPath = $repository; HeadCommit = $head }
}

function Set-OwnerSystemPrivateAcl {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][bool]$Directory)
    $ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $systemSid = New-Object Security.Principal.SecurityIdentifier("S-1-5-18")
    if ($Directory) {
        $security = New-Object Security.AccessControl.DirectorySecurity
        $inheritance = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit
    } else {
        $security = New-Object Security.AccessControl.FileSecurity
        $inheritance = [Security.AccessControl.InheritanceFlags]::None
    }
    $security.SetOwner($ownerSid)
    $security.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($ownerSid, $systemSid)) {
        $rule = New-Object Security.AccessControl.FileSystemAccessRule(
            $sid,
            [Security.AccessControl.FileSystemRights]::FullControl,
            $inheritance,
            [Security.AccessControl.PropagationFlags]::None,
            [Security.AccessControl.AccessControlType]::Allow
        )
        [void]$security.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $security
}

function Assert-OwnerSystemPrivateAcl {
    param([Parameter(Mandatory = $true)][string]$Path)
    $acl = Get-Acl -LiteralPath $Path
    $ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $expected = @($ownerSid, "S-1-5-18")
    $rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    if (-not $acl.AreAccessRulesProtected -or $rules.Count -ne 2) {
        throw "The onboarding plan ACL was not limited to owner and SYSTEM."
    }
    $seen = @{}
    foreach ($rule in $rules) {
        $sid = $rule.IdentityReference.Value
        if (
            $expected -cnotcontains $sid -or $seen.ContainsKey($sid) -or $rule.IsInherited -or
            $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
            ($rule.FileSystemRights -band [Security.AccessControl.FileSystemRights]::FullControl) -ne [Security.AccessControl.FileSystemRights]::FullControl
        ) {
            throw "The onboarding plan ACL was not the exact owner/SYSTEM private ACL."
        }
        $seen[$sid] = $true
    }
    if (-not $seen.ContainsKey($ownerSid) -or -not $seen.ContainsKey("S-1-5-18")) {
        throw "The onboarding plan ACL principal set was incomplete."
    }
}

function Resolve-PlanDirectory {
    param([bool]$Create)
    $data = [IO.Path]::GetFullPath($DataDir)
    Assert-NoReparseComponents $data $false
    if (-not (Test-Path -LiteralPath $data -PathType Container)) {
        throw "The Windows-local master data directory is unavailable."
    }
    $directory = Join-Path $data $planDirectoryLeaf
    Assert-NoReparseComponents $directory $true
    if (-not (Test-Path -LiteralPath $directory)) {
        if (-not $Create) { throw "The onboarding plan directory is unavailable." }
        [void](New-Item -ItemType Directory -Path $directory)
        Set-OwnerSystemPrivateAcl $directory $true
    }
    Assert-NoReparseComponents $directory $false
    Assert-OwnerSystemPrivateAcl $directory
    return $directory
}

function Resolve-PlanPaths {
    param([Parameter(Mandatory = $true)][string]$Identifier, [bool]$CreateDirectory)
    if ($Identifier -cnotmatch $uuidPattern) { throw "The onboarding plan ID was not a canonical UUID." }
    $directory = Resolve-PlanDirectory $CreateDirectory
    return [ordered]@{
        Plan = Join-Path $directory "$Identifier.plan.json"
        Receipt = Join-Path $directory "$Identifier.receipt.json"
    }
}

function Get-ScopeDocument {
    param([string]$RepositoryId, [string]$Path, [string]$HeadCommit)
    return [ordered]@{
        expected_base_branch = $baseBranch
        expected_head_commit = $HeadCommit
        repository_id = $RepositoryId
        repository_path = $Path
    }
}

function Get-ApprovalPlanDocument {
    param($Plan)
    return [ordered]@{
        schema_version = [UInt64]$Plan.schema_version
        plan_id = [string]$Plan.plan_id
        repository_id = [string]$Plan.repository_id
        repository_path = [string]$Plan.repository_path
        base_branch = [string]$Plan.base_branch
        head_commit = [string]$Plan.head_commit
        created_at_ms = [UInt64]$Plan.created_at_ms
        expires_at_ms = [UInt64]$Plan.expires_at_ms
        scope_sha256 = [string]$Plan.scope_sha256
    }
}

function Get-GrantBindingHex {
    param([string]$Purpose, [string]$ApprovalPlanSha256, [string]$Kind)
    return Get-Utf8Sha256Hex "assemblywright.repository-onboarding.$Purpose.v1`0$ApprovalPlanSha256`0$Kind"
}

function Get-CanonicalPlanLine {
    param($Plan)
    $canonical = [ordered]@{
        schema_version = [UInt64]$Plan.schema_version
        status = [string]$Plan.status
        plan_id = [string]$Plan.plan_id
        repository_id = [string]$Plan.repository_id
        repository_path = [string]$Plan.repository_path
        base_branch = [string]$Plan.base_branch
        head_commit = [string]$Plan.head_commit
        created_at_ms = [UInt64]$Plan.created_at_ms
        expires_at_ms = [UInt64]$Plan.expires_at_ms
        scope_sha256 = [string]$Plan.scope_sha256
        approval_plan_sha256 = [string]$Plan.approval_plan_sha256
        registration_scope_sha256 = [string]$Plan.registration_scope_sha256
        registration_owner_approval_sha256 = [string]$Plan.registration_owner_approval_sha256
        cloud_disclosure_scope_sha256 = [string]$Plan.cloud_disclosure_scope_sha256
        cloud_disclosure_owner_approval_sha256 = [string]$Plan.cloud_disclosure_owner_approval_sha256
        autonomous_publication_scope_sha256 = [string]$Plan.autonomous_publication_scope_sha256
        autonomous_publication_owner_approval_sha256 = [string]$Plan.autonomous_publication_owner_approval_sha256
    }
    return $canonical | ConvertTo-Json -Compress
}

function Assert-PlanDocument {
    param($Plan, [string]$ExpectedIdentifier)
    Assert-ExactKeys $Plan @(
        "schema_version", "status", "plan_id", "repository_id", "repository_path", "base_branch", "head_commit",
        "created_at_ms", "expires_at_ms", "scope_sha256", "approval_plan_sha256",
        "registration_scope_sha256", "registration_owner_approval_sha256",
        "cloud_disclosure_scope_sha256", "cloud_disclosure_owner_approval_sha256",
        "autonomous_publication_scope_sha256", "autonomous_publication_owner_approval_sha256"
    ) "Repository-onboarding plan"
    if (
        [UInt64]$Plan.schema_version -ne $planSchemaVersion -or [string]$Plan.status -cne $planStatus -or
        [string]$Plan.plan_id -cne $ExpectedIdentifier -or [string]$Plan.plan_id -cnotmatch $uuidPattern -or
        [string]$Plan.repository_id -cnotmatch $uuidPattern -or [string]$Plan.base_branch -cne $baseBranch -or
        [string]$Plan.head_commit -cnotmatch $commitPattern -or [UInt64]$Plan.created_at_ms -eq 0 -or
        [UInt64]$Plan.expires_at_ms -ne ([UInt64]$Plan.created_at_ms + $planLifetimeMs)
    ) { throw "The repository-onboarding plan binding was invalid." }
    foreach ($digest in @(
        $Plan.scope_sha256, $Plan.approval_plan_sha256, $Plan.registration_scope_sha256,
        $Plan.registration_owner_approval_sha256, $Plan.cloud_disclosure_scope_sha256,
        $Plan.cloud_disclosure_owner_approval_sha256, $Plan.autonomous_publication_scope_sha256,
        $Plan.autonomous_publication_owner_approval_sha256
    )) {
        if ([string]$digest -cnotmatch $shaPattern) { throw "The repository-onboarding plan contained a malformed digest." }
    }
    $scope = Get-ScopeDocument ([string]$Plan.repository_id) ([string]$Plan.repository_path) ([string]$Plan.head_commit)
    $scopeSha256 = Get-Utf8Sha256Hex ($scope | ConvertTo-Json -Compress)
    $approvalPlanSha256 = Get-Utf8Sha256Hex ((Get-ApprovalPlanDocument $Plan) | ConvertTo-Json -Compress)
    if (
        [string]$Plan.scope_sha256 -cne $scopeSha256 -or [string]$Plan.approval_plan_sha256 -cne $approvalPlanSha256 -or
        [string]$Plan.registration_scope_sha256 -cne $scopeSha256 -or
        [string]$Plan.registration_owner_approval_sha256 -cne (Get-GrantBindingHex "owner-approval" $approvalPlanSha256 "registration") -or
        [string]$Plan.cloud_disclosure_scope_sha256 -cne (Get-GrantBindingHex "scope" $approvalPlanSha256 "cloud_disclosure") -or
        [string]$Plan.cloud_disclosure_owner_approval_sha256 -cne (Get-GrantBindingHex "owner-approval" $approvalPlanSha256 "cloud_disclosure") -or
        [string]$Plan.autonomous_publication_scope_sha256 -cne (Get-GrantBindingHex "scope" $approvalPlanSha256 "autonomous_publication") -or
        [string]$Plan.autonomous_publication_owner_approval_sha256 -cne (Get-GrantBindingHex "owner-approval" $approvalPlanSha256 "autonomous_publication")
    ) { throw "The repository-onboarding plan canonical digest binding drifted." }
}

function Write-PrivateFileAtomically {
    param([string]$Path, [string]$Text)
    $directory = [IO.Path]::GetDirectoryName($Path)
    Assert-NoReparseComponents $directory $false
    Assert-OwnerSystemPrivateAcl $directory
    if (Test-Path -LiteralPath $Path) { throw "The bounded onboarding publication target already exists." }
    $temporaryPath = Join-Path $directory ".$([IO.Path]::GetFileName($Path)).$([Guid]::NewGuid().ToString('N')).tmp"
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    if ($bytes.Length -eq 0 -or $bytes.Length -gt $maximumPlanBytes) { throw "The bounded onboarding document size was invalid." }
    $stream = $null
    try {
        $stream = [IO.FileStream]::new($temporaryPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None, 4096, [IO.FileOptions]::WriteThrough)
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
        $stream.Dispose()
        $stream = $null
        Set-OwnerSystemPrivateAcl $temporaryPath $false
        Assert-OwnerSystemPrivateAcl $temporaryPath
        [IO.File]::Move($temporaryPath, $Path)
        Assert-OwnerSystemPrivateAcl $Path
    } finally {
        if ($null -ne $stream) { $stream.Dispose() }
        if (Test-Path -LiteralPath $temporaryPath) { Remove-Item -LiteralPath $temporaryPath -Force }
    }
}

function Read-PrivateJsonDocument {
    param([string]$Path)
    Assert-NoReparseComponents $Path $false
    Assert-OwnerSystemPrivateAcl $Path
    $held = [Assemblywright.HeldOnboardingFile]::Open([IO.Path]::GetFullPath($Path))
    try {
        $held.RevalidatePath([IO.Path]::GetFullPath($Path))
        $bytes = $held.ReadAll($maximumPlanBytes)
        $utf8 = New-Object Text.UTF8Encoding($false, $true)
        $text = $utf8.GetString($bytes)
        if ($text.Contains("`r") -or $text.Contains("`n")) { throw "The onboarding document was not one canonical JSON line." }
        try { $value = $text | ConvertFrom-Json }
        catch { throw "The onboarding document JSON was malformed." }
        $held.RevalidatePath([IO.Path]::GetFullPath($Path))
        Assert-OwnerSystemPrivateAcl $Path
        return [ordered]@{ Value = $value; Text = $text }
    } finally { $held.Dispose() }
}

function Read-Plan {
    param([string]$Identifier, [bool]$AllowExpired)
    $paths = Resolve-PlanPaths $Identifier $false
    $read = Read-PrivateJsonDocument $paths.Plan
    Assert-PlanDocument $read.Value $Identifier
    if ((Get-CanonicalPlanLine $read.Value) -cne $read.Text) { throw "The repository-onboarding plan was not canonically encoded." }
    if (-not $AllowExpired -and [UInt64]$read.Value.expires_at_ms -lt (Get-UtcMilliseconds)) {
        throw "The repository-onboarding plan expired; create a new plan."
    }
    return [ordered]@{ Plan = $read.Value; Paths = $paths }
}

function Read-OwnerToken {
    $tokenPath = Join-Path ([IO.Path]::GetFullPath($DataDir)) "development.token"
    Assert-NoReparseComponents $tokenPath $false
    $held = [Assemblywright.HeldOnboardingFile]::Open($tokenPath)
    try {
        $bytes = $held.ReadAll($maximumTokenBytes)
        if (@($bytes | Where-Object { [int]$_ -gt 127 }).Count -ne 0) { throw "The Windows-local owner token was invalid." }
        $raw = [Text.Encoding]::ASCII.GetString($bytes)
        $token = if ($raw.EndsWith("`n", [StringComparison]::Ordinal)) { $raw.Substring(0, $raw.Length - 1) } else { $raw }
        if ($token.Length -lt 32 -or $token.Length -gt 256 -or $token -notmatch '^[\x21-\x7e]+$') {
            throw "The Windows-local owner token was invalid."
        }
        return $token
    } finally { $held.Dispose() }
}

function Open-OwnerLoopback {
    $token = Read-OwnerToken
    $script:ownerHeaders = @{ Authorization = "Bearer $token" }
    $script:baseUri = "http://$Endpoint"
    $token = $null
}

function Close-OwnerLoopback {
    $script:ownerHeaders = $null
    $script:baseUri = $null
}

function Invoke-ExactGet {
    param([string]$Path)
    try { return Invoke-RestMethod -Method Get -Uri "$script:baseUri$Path" -Headers $script:ownerHeaders }
    catch { throw "The Windows-local owner read failed closed." }
}

function Invoke-ExactPost {
    param([string]$Path, $Body)
    $parameters = @{
        Method = "Post"; Uri = "$script:baseUri$Path"; Headers = $script:ownerHeaders
        ContentType = "application/json"; Body = ($Body | ConvertTo-Json -Compress -Depth 10)
    }
    try { return Invoke-RestMethod @parameters }
    catch { throw "The Windows-local owner mutation response was not accepted; rerun Check before any deliberate retry." }
}

function Get-GrantSet {
    param([string]$RepositoryId)
    $set = Invoke-ExactGet "/v1/feature-conveyor/repositories/$RepositoryId/grants"
    Assert-ExactKeys $set @("schema_version", "repository_id", "emergency_paused", "emergency_pause_revision", "registration", "cloud_disclosure", "autonomous_publication") "Repository-grant projection"
    if ([UInt64]$set.schema_version -ne $ownerControlSchemaVersion -or [string]$set.repository_id -cne $RepositoryId -or $set.emergency_paused -isnot [bool]) {
        throw "The repository-grant projection binding was invalid."
    }
    return $set
}

function Assert-GrantSetPauseEpoch {
    param($Set, [UInt64]$ExpectedPauseRevision)
    if ([bool]$Set.emergency_paused -or [UInt64]$Set.emergency_pause_revision -ne $ExpectedPauseRevision) {
        throw "The repository-onboarding Emergency Pause epoch changed."
    }
}

function Get-PlanGrantBinding {
    param($Plan, [string]$Kind)
    switch ($Kind) {
        "registration" { return [ordered]@{ Scope = [string]$Plan.registration_scope_sha256; Approval = [string]$Plan.registration_owner_approval_sha256 } }
        "cloud_disclosure" { return [ordered]@{ Scope = [string]$Plan.cloud_disclosure_scope_sha256; Approval = [string]$Plan.cloud_disclosure_owner_approval_sha256 } }
        "autonomous_publication" { return [ordered]@{ Scope = [string]$Plan.autonomous_publication_scope_sha256; Approval = [string]$Plan.autonomous_publication_owner_approval_sha256 } }
        default { throw "The repository-grant kind was unsupported." }
    }
}

function Assert-ExactCurrentGrant {
    param($Grant, $Plan, [string]$Kind)
    if ($null -eq $Grant) { return $false }
    Assert-ExactKeys $Grant @("revision", "scope_sha256", "owner_approval_sha256", "expires_at_ms", "revoked", "active") "Current repository grant"
    $binding = Get-PlanGrantBinding $Plan $Kind
    if (
        [UInt64]$Grant.revision -ne 1 -or $Grant.revoked -ne $false -or $Grant.active -ne $true -or
        $null -ne $Grant.expires_at_ms -or (Convert-BytesToHex $Grant.scope_sha256) -cne $binding.Scope -or
        (Convert-BytesToHex $Grant.owner_approval_sha256) -cne $binding.Approval
    ) { throw "The current repository grant was not the exact resumable revision-1 plan binding." }
    return $true
}

function Record-MissingGrant {
    param($Set, $Plan, [string]$Kind, [UInt64]$ExpectedPauseRevision)
    Assert-GrantSetPauseEpoch $Set $ExpectedPauseRevision
    $binding = Get-PlanGrantBinding $Plan $Kind
    $request = [ordered]@{
        schema_version = $ownerControlSchemaVersion
        expected_current_revision = 0
        expected_emergency_pause_revision = $ExpectedPauseRevision
        grant = [ordered]@{
            repository_id = [string]$Plan.repository_id
            kind = $Kind
            revision = 1
            scope_sha256 = @(Convert-HexToBytes $binding.Scope)
            owner_approval_sha256 = @(Convert-HexToBytes $binding.Approval)
            expires_at_ms = $null
            revoked = $false
        }
    }
    $receipt = Invoke-ExactPost "/v1/feature-conveyor/repository-grants" $request
    Assert-ExactKeys $receipt @("schema_version", "repository_id", "kind", "revision", "scope_sha256", "owner_approval_sha256", "expires_at_ms", "revoked", "emergency_pause_revision", "status") "Repository-grant receipt"
    if (
        [UInt64]$receipt.schema_version -ne $ownerControlSchemaVersion -or [string]$receipt.repository_id -cne [string]$Plan.repository_id -or
        [string]$receipt.kind -cne $Kind -or [UInt64]$receipt.revision -ne 1 -or [string]$receipt.status -cne "recorded" -or
        $receipt.revoked -ne $false -or $null -ne $receipt.expires_at_ms -or
        [UInt64]$receipt.emergency_pause_revision -ne $ExpectedPauseRevision -or
        (Convert-BytesToHex $receipt.scope_sha256) -cne $binding.Scope -or
        (Convert-BytesToHex $receipt.owner_approval_sha256) -cne $binding.Approval
    ) { throw "The repository-grant receipt did not bind the exact approved plan." }
}

function Get-PreflightFingerprintHex {
    param($Receipt)
    $stream = New-Object IO.MemoryStream
    try {
        $write = {
            param([byte[]]$Bytes)
            $stream.Write($Bytes, 0, $Bytes.Length)
        }
        & $write ([Text.Encoding]::UTF8.GetBytes("assemblywright.repository-preflight.v1`0"))
        $uuidHex = ([string]$Receipt.repository_id).Replace("-", "")
        $uuidBytes = New-Object byte[] 16
        for ($index = 0; $index -lt 16; $index += 1) { $uuidBytes[$index] = [Convert]::ToByte($uuidHex.Substring($index * 2, 2), 16) }
        & $write $uuidBytes
        foreach ($number in @([UInt64]$Receipt.registration_grant_revision)) { & $write (Get-UInt64BigEndian $number) }
        & $write (Convert-HexToBytes ([string]$Receipt.scope_sha256))
        & $write (Get-UInt64BigEndian ([UInt64]$Receipt.emergency_pause_revision))
        foreach ($text in @([string]$Receipt.base_branch, [string]$Receipt.head_commit)) {
            $bytes = [Text.Encoding]::UTF8.GetBytes($text)
            & $write (Get-UInt64BigEndian ([UInt64]$bytes.Length))
            & $write $bytes
        }
        & $write (Get-UInt64BigEndian ([UInt64]$Receipt.observed_at_ms))
        return Convert-BytesToHex (Get-Sha256Bytes $stream.ToArray())
    } finally { $stream.Dispose() }
}

function Get-UInt64BigEndian {
    param([UInt64]$Value)
    $bytes = [BitConverter]::GetBytes($Value)
    if ([BitConverter]::IsLittleEndian) { [Array]::Reverse($bytes) }
    return $bytes
}

function Assert-PreflightReceipt {
    param($Receipt, $Plan, [UInt64]$ExpectedPauseRevision)
    Assert-ExactKeys $Receipt @("schema_version", "repository_id", "registration_grant_revision", "scope_sha256", "emergency_pause_revision", "base_branch", "head_commit", "preflight_fingerprint_sha256", "observed_at_ms", "status") "Repository-preflight receipt"
    $fingerprint = Convert-BytesToHex $Receipt.preflight_fingerprint_sha256
    $normalized = [ordered]@{
        repository_id = [string]$Receipt.repository_id
        registration_grant_revision = [UInt64]$Receipt.registration_grant_revision
        scope_sha256 = Convert-BytesToHex $Receipt.scope_sha256
        emergency_pause_revision = [UInt64]$Receipt.emergency_pause_revision
        base_branch = [string]$Receipt.base_branch
        head_commit = [string]$Receipt.head_commit
        observed_at_ms = [UInt64]$Receipt.observed_at_ms
    }
    if (
        [UInt64]$Receipt.schema_version -ne $ownerControlSchemaVersion -or [string]$Receipt.status -cne "identity_eligible" -or
        [string]$Receipt.repository_id -cne [string]$Plan.repository_id -or [UInt64]$Receipt.registration_grant_revision -ne 1 -or
        $normalized.scope_sha256 -cne [string]$Plan.scope_sha256 -or [UInt64]$Receipt.emergency_pause_revision -ne $ExpectedPauseRevision -or
        [string]$Receipt.base_branch -cne $baseBranch -or [string]$Receipt.head_commit -cne [string]$Plan.head_commit -or
        [UInt64]$Receipt.observed_at_ms -eq 0 -or [UInt64]$Receipt.observed_at_ms -gt (Get-UtcMilliseconds) -or
        $fingerprint -cnotmatch $shaPattern -or $fingerprint -cne (Get-PreflightFingerprintHex $normalized)
    ) { throw "The repository-preflight receipt did not bind the exact approved plan." }
    return $fingerprint
}

function Get-CanonicalAuthoringReceiptLine {
    param($Receipt)
    $canonical = [ordered]@{
        schema_version = [UInt64]$Receipt.schema_version
        status = [string]$Receipt.status
        repository_id = [string]$Receipt.repository_id
        registration_grant_revision = [UInt64]$Receipt.registration_grant_revision
        cloud_disclosure_grant_revision = [UInt64]$Receipt.cloud_disclosure_grant_revision
        autonomous_publication_grant_revision = [UInt64]$Receipt.autonomous_publication_grant_revision
        base_branch = [string]$Receipt.base_branch
        head_commit = [string]$Receipt.head_commit
        scope_sha256 = [string]$Receipt.scope_sha256
        approval_plan_sha256 = [string]$Receipt.approval_plan_sha256
        preflight_fingerprint_sha256 = [string]$Receipt.preflight_fingerprint_sha256
    }
    return $canonical | ConvertTo-Json -Compress
}

function Assert-AuthoringReceipt {
    param($Receipt, $Plan)
    Assert-ExactKeys $Receipt @("schema_version", "status", "repository_id", "registration_grant_revision", "cloud_disclosure_grant_revision", "autonomous_publication_grant_revision", "base_branch", "head_commit", "scope_sha256", "approval_plan_sha256", "preflight_fingerprint_sha256") "Repository-onboarding authoring receipt"
    if (
        [UInt64]$Receipt.schema_version -ne 1 -or [string]$Receipt.status -cne $receiptStatus -or
        [string]$Receipt.repository_id -cne [string]$Plan.repository_id -or
        [UInt64]$Receipt.registration_grant_revision -ne 1 -or [UInt64]$Receipt.cloud_disclosure_grant_revision -ne 1 -or
        [UInt64]$Receipt.autonomous_publication_grant_revision -ne 1 -or [string]$Receipt.base_branch -cne $baseBranch -or
        [string]$Receipt.head_commit -cne [string]$Plan.head_commit -or [string]$Receipt.scope_sha256 -cne [string]$Plan.scope_sha256 -or
        [string]$Receipt.approval_plan_sha256 -cne [string]$Plan.approval_plan_sha256 -or
        [string]$Receipt.preflight_fingerprint_sha256 -cnotmatch $shaPattern
    ) { throw "The repository-onboarding authoring receipt binding was invalid." }
}

function Get-GrantState {
    param($Set, $Plan)
    $present = 0
    foreach ($kind in @("registration", "cloud_disclosure", "autonomous_publication")) {
        if (Assert-ExactCurrentGrant $Set.$kind $Plan $kind) { $present += 1 }
    }
    if ($present -eq 0) { return "absent" }
    if ($present -eq 3) { return "exact_revision_1" }
    return "exact_partial_revision_1"
}

function Get-ApprovePlanDisposition {
    param(
        $Plan,
        [ValidateSet("absent", "exact_partial_revision_1", "exact_revision_1")]
        [string]$GrantState,
        [bool]$ReceiptPresent,
        [UInt64]$NowMs
    )
    if ([UInt64]$Plan.expires_at_ms -ge $NowMs) { return "active" }
    if (-not $ReceiptPresent -and $GrantState -ceq "exact_partial_revision_1") {
        return "expired_partial_resume"
    }
    if (-not $ReceiptPresent -and $GrantState -ceq "exact_revision_1") {
        return "expired_complete_resume"
    }
    if ($ReceiptPresent -and $GrantState -ceq "exact_revision_1") {
        return "expired_receipt_replay"
    }
    throw "The expired repository-onboarding plan may only resume existing exact revision-1 grants or replay its exact stored receipt and grants."
}

if ($Action -eq "SelfTest") {
    if ($RepositoryPath.Length -ne 0 -or $PlanId.Length -ne 0 -or $ConfirmRegistration -or $ConfirmCloudDisclosure -or $ConfirmAutonomousPublication) {
        throw "SelfTest accepts no repository, plan, or approval input."
    }
    $extraRejected = $false
    try { Assert-ExactKeys ([ordered]@{ schema_version = 1; status = "ok"; path = "forbidden" }) @("schema_version", "status") "Self-test extra-key regression" }
    catch { $extraRejected = $_.Exception.Message -ceq "Self-test extra-key regression had an unexpected JSON shape." }
    $revisionRejected = $false
    try {
        [void](Assert-ExactCurrentGrant ([ordered]@{ revision = 2; scope_sha256 = @(1..32); owner_approval_sha256 = @(1..32); expires_at_ms = $null; revoked = $false; active = $true }) ([ordered]@{ registration_scope_sha256 = ("01" * 32); registration_owner_approval_sha256 = ("01" * 32) }) "registration")
    } catch { $revisionRejected = $true }
    $revokedGrantRejected = $false
    try {
        [void](Assert-ExactCurrentGrant ([ordered]@{ revision = 1; scope_sha256 = @(1..32 | ForEach-Object { 1 }); owner_approval_sha256 = @(1..32 | ForEach-Object { 1 }); expires_at_ms = $null; revoked = $true; active = $false }) ([ordered]@{ registration_scope_sha256 = ("01" * 32); registration_owner_approval_sha256 = ("01" * 32) }) "registration")
    } catch { $revokedGrantRejected = $true }
    $foreignGrantRejected = $false
    try {
        [void](Assert-ExactCurrentGrant ([ordered]@{ revision = 1; scope_sha256 = @(1..32 | ForEach-Object { 2 }); owner_approval_sha256 = @(1..32 | ForEach-Object { 1 }); expires_at_ms = $null; revoked = $false; active = $true }) ([ordered]@{ registration_scope_sha256 = ("01" * 32); registration_owner_approval_sha256 = ("01" * 32) }) "registration")
    } catch { $foreignGrantRejected = $true }
    $domainA = Get-GrantBindingHex "scope" ("11" * 32) "cloud_disclosure"
    $domainB = Get-GrantBindingHex "scope" ("11" * 32) "autonomous_publication"
    $pauseEpochChurnRejected = $false
    try {
        Assert-GrantSetPauseEpoch ([ordered]@{ emergency_paused = $false; emergency_pause_revision = 8 }) 7
    } catch {
        $pauseEpochChurnRejected = $_.Exception.Message -ceq "The repository-onboarding Emergency Pause epoch changed."
    }
    $expiredPlan = [ordered]@{ expires_at_ms = [UInt64]100 }
    $freshExpiryDisposition = Get-ApprovePlanDisposition $expiredPlan "absent" $false ([UInt64]100)
    $expiredAbsentRejected = $false
    try { [void](Get-ApprovePlanDisposition $expiredPlan "absent" $false ([UInt64]101)) }
    catch {
        $expiredAbsentRejected = $_.Exception.Message -ceq "The expired repository-onboarding plan may only resume existing exact revision-1 grants or replay its exact stored receipt and grants."
    }
    $expiredCompleteDisposition = Get-ApprovePlanDisposition $expiredPlan "exact_revision_1" $false ([UInt64]101)
    $expiredReceiptWithPartialRejected = $false
    try { [void](Get-ApprovePlanDisposition $expiredPlan "exact_partial_revision_1" $true ([UInt64]101)) }
    catch { $expiredReceiptWithPartialRejected = $true }
    $expiredPartialDisposition = Get-ApprovePlanDisposition $expiredPlan "exact_partial_revision_1" $false ([UInt64]101)
    $expiredReplayDisposition = Get-ApprovePlanDisposition $expiredPlan "exact_revision_1" $true ([UInt64]101)
    $metadataRoot = Join-Path ([IO.Path]::GetTempPath()) "assemblywright-onboarding-metadata-$([Guid]::NewGuid().ToString('N'))"
    $gitMetadata = Join-Path $metadataRoot ".git"
    $rootGitmodulesRejected = $false
    $configWorktreeRejected = $false
    try {
        [void](New-Item -ItemType Directory -Path $metadataRoot)
        [void](New-Item -ItemType Directory -Path $gitMetadata)
        [IO.File]::WriteAllText((Join-Path $metadataRoot ".gitmodules"), "[submodule]`n")
        try { Assert-NoUnsupportedRepositoryMetadata $metadataRoot $gitMetadata }
        catch { $rootGitmodulesRejected = $_.Exception.Message -ceq "Worktree, linked, or submodule Git metadata is not eligible for onboarding." }
        Remove-Item -LiteralPath (Join-Path $metadataRoot ".gitmodules") -Force
        [IO.File]::WriteAllText((Join-Path $gitMetadata "config.worktree"), "[core]`n")
        try { Assert-NoUnsupportedRepositoryMetadata $metadataRoot $gitMetadata }
        catch { $configWorktreeRejected = $_.Exception.Message -ceq "Worktree, linked, or submodule Git metadata is not eligible for onboarding." }
    } finally {
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
        $metadataParent = [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($metadataRoot)).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
        if ($metadataParent -cne $tempRoot -or [IO.Path]::GetFileName($metadataRoot) -cnotmatch '^assemblywright-onboarding-metadata-[0-9a-f]{32}$') {
            throw "The repository-metadata self-test temporary path escaped its bounded leaf."
        }
        if (Test-Path -LiteralPath $metadataRoot) { Remove-Item -LiteralPath $metadataRoot -Recurse -Force }
    }
    if (
        -not $extraRejected -or -not $revisionRejected -or -not $revokedGrantRejected -or
        -not $foreignGrantRejected -or $domainA -ceq $domainB -or
        -not $pauseEpochChurnRejected -or $freshExpiryDisposition -cne "active" -or
        -not $expiredAbsentRejected -or $expiredCompleteDisposition -cne "expired_complete_resume" -or
        -not $expiredReceiptWithPartialRejected -or $expiredPartialDisposition -cne "expired_partial_resume" -or
        $expiredReplayDisposition -cne "expired_receipt_replay" -or
        -not $rootGitmodulesRejected -or -not $configWorktreeRejected
    ) { throw "Repository-onboarding self-test failed." }
    '{"approval_domain_separation":"verified","config_worktree_negative":"verified","exact_grant_drift_negative":"verified","exact_shape_negative":"verified","expired_absent_approval_negative":"verified","expired_complete_resume":"verified","expired_partial_resume":"verified","expired_receipt_replay":"verified","pause_epoch_churn_negative":"verified","revoked_grant_negative":"verified","revision_resume_negative":"verified","root_gitmodules_negative":"verified","schema_version":1,"status":"repository_onboarding_self_test_passed"}'
    exit 0
}

if ($Action -eq "Plan") {
    if ($RepositoryPath.Length -eq 0 -or $PlanId.Length -ne 0 -or $ConfirmRegistration -or $ConfirmCloudDisclosure -or $ConfirmAutonomousPublication) {
        throw "Plan requires only -RepositoryPath and accepts no approval switches."
    }
    $eligible = Assert-RepositoryEligible $RepositoryPath
    $identifier = [Guid]::NewGuid().ToString().ToLowerInvariant()
    $repositoryId = [Guid]::NewGuid().ToString().ToLowerInvariant()
    $createdAt = Get-UtcMilliseconds
    $scope = Get-ScopeDocument $repositoryId $eligible.RepositoryPath $eligible.HeadCommit
    $scopeSha256 = Get-Utf8Sha256Hex ($scope | ConvertTo-Json -Compress)
    $draft = [ordered]@{
        schema_version = $planSchemaVersion; plan_id = $identifier; repository_id = $repositoryId
        repository_path = $eligible.RepositoryPath; base_branch = $baseBranch; head_commit = $eligible.HeadCommit
        created_at_ms = $createdAt; expires_at_ms = [UInt64]($createdAt + $planLifetimeMs); scope_sha256 = $scopeSha256
    }
    $approvalPlanSha256 = Get-Utf8Sha256Hex ($draft | ConvertTo-Json -Compress)
    $plan = [ordered]@{
        schema_version = $planSchemaVersion; status = $planStatus; plan_id = $identifier; repository_id = $repositoryId
        repository_path = $eligible.RepositoryPath; base_branch = $baseBranch; head_commit = $eligible.HeadCommit
        created_at_ms = $createdAt; expires_at_ms = [UInt64]($createdAt + $planLifetimeMs); scope_sha256 = $scopeSha256
        approval_plan_sha256 = $approvalPlanSha256
        registration_scope_sha256 = $scopeSha256
        registration_owner_approval_sha256 = Get-GrantBindingHex "owner-approval" $approvalPlanSha256 "registration"
        cloud_disclosure_scope_sha256 = Get-GrantBindingHex "scope" $approvalPlanSha256 "cloud_disclosure"
        cloud_disclosure_owner_approval_sha256 = Get-GrantBindingHex "owner-approval" $approvalPlanSha256 "cloud_disclosure"
        autonomous_publication_scope_sha256 = Get-GrantBindingHex "scope" $approvalPlanSha256 "autonomous_publication"
        autonomous_publication_owner_approval_sha256 = Get-GrantBindingHex "owner-approval" $approvalPlanSha256 "autonomous_publication"
    }
    Assert-PlanDocument $plan $identifier
    $paths = Resolve-PlanPaths $identifier $true
    Write-PrivateFileAtomically $paths.Plan (Get-CanonicalPlanLine $plan)
    [ordered]@{
        schema_version = 1; status = "repository_onboarding_planned"; plan_id = $identifier; repository_id = $repositoryId
        base_branch = $baseBranch; head_commit = $eligible.HeadCommit; scope_sha256 = $scopeSha256
        approval_plan_sha256 = $approvalPlanSha256; expires_at_ms = [UInt64]$plan.expires_at_ms
    } | ConvertTo-Json -Compress
    exit 0
}

if ($RepositoryPath.Length -ne 0 -or $PlanId -cnotmatch $uuidPattern) {
    throw "$Action requires one canonical -PlanId and accepts no repository path."
}
$loaded = Read-Plan $PlanId $true
$plan = $loaded.Plan
[void](Assert-RepositoryEligible ([string]$plan.repository_path) ([string]$plan.head_commit))

if ($Action -eq "Check") {
    if ($ConfirmRegistration -or $ConfirmCloudDisclosure -or $ConfirmAutonomousPublication) { throw "Check accepts no approval switches." }
    Open-OwnerLoopback
    try {
        $set = Get-GrantSet ([string]$plan.repository_id)
        $grantState = Get-GrantState $set $plan
    } finally { Close-OwnerLoopback }
    $receiptPresent = Test-Path -LiteralPath $loaded.Paths.Receipt -PathType Leaf
    if ($receiptPresent) {
        $stored = Read-PrivateJsonDocument $loaded.Paths.Receipt
        Assert-AuthoringReceipt $stored.Value $plan
        if ((Get-CanonicalAuthoringReceiptLine $stored.Value) -cne $stored.Text -or $grantState -cne "exact_revision_1") {
            throw "The stored onboarding receipt or current grant state drifted."
        }
    }
    [ordered]@{
        schema_version = 1; status = "repository_onboarding_check_passed"; plan_id = $PlanId
        repository_id = [string]$plan.repository_id; grant_state = $grantState; authoring_receipt_present = $receiptPresent
        head_commit = [string]$plan.head_commit; approval_plan_sha256 = [string]$plan.approval_plan_sha256
    } | ConvertTo-Json -Compress
    exit 0
}

if (-not $ConfirmRegistration -or -not $ConfirmCloudDisclosure -or -not $ConfirmAutonomousPublication) {
    throw "Approve requires separate -ConfirmRegistration, -ConfirmCloudDisclosure, and -ConfirmAutonomousPublication switches."
}

Open-OwnerLoopback
try {
    $set = Get-GrantSet ([string]$plan.repository_id)
    $initialPauseRevision = [UInt64]$set.emergency_pause_revision
    Assert-GrantSetPauseEpoch $set $initialPauseRevision
    $receiptPresent = Test-Path -LiteralPath $loaded.Paths.Receipt -PathType Leaf
    $grantState = Get-GrantState $set $plan
    [void](Get-ApprovePlanDisposition $plan $grantState $receiptPresent (Get-UtcMilliseconds))
    if ($receiptPresent) {
        Assert-GrantSetPauseEpoch $set $initialPauseRevision
        if ($grantState -cne "exact_revision_1") { throw "The stored onboarding receipt no longer had exact current grants." }
        $stored = Read-PrivateJsonDocument $loaded.Paths.Receipt
        Assert-AuthoringReceipt $stored.Value $plan
        if ((Get-CanonicalAuthoringReceiptLine $stored.Value) -cne $stored.Text) { throw "The stored onboarding receipt was not canonically encoded." }
        $stored.Text
        exit 0
    }

    foreach ($kind in @("registration", "cloud_disclosure", "autonomous_publication")) {
        Assert-GrantSetPauseEpoch $set $initialPauseRevision
        $current = $set.$kind
        if (-not (Assert-ExactCurrentGrant $current $plan $kind)) {
            [void](Assert-RepositoryEligible ([string]$plan.repository_path) ([string]$plan.head_commit))
            Record-MissingGrant $set $plan $kind $initialPauseRevision
        }
        $set = Get-GrantSet ([string]$plan.repository_id)
        Assert-GrantSetPauseEpoch $set $initialPauseRevision
        [void](Assert-ExactCurrentGrant $set.$kind $plan $kind)
    }
    Assert-GrantSetPauseEpoch $set $initialPauseRevision
    if ((Get-GrantState $set $plan) -cne "exact_revision_1") {
        throw "Repository onboarding did not reach three exact active revision-1 grants."
    }
    [void](Assert-RepositoryEligible ([string]$plan.repository_path) ([string]$plan.head_commit))
    $scope = Get-ScopeDocument ([string]$plan.repository_id) ([string]$plan.repository_path) ([string]$plan.head_commit)
    $preflightRequest = [ordered]@{
        schema_version = $ownerControlSchemaVersion
        scope = $scope
        scope_sha256 = @(Convert-HexToBytes ([string]$plan.scope_sha256))
        registration_grant_revision = 1
        expected_emergency_pause_revision = $initialPauseRevision
    }
    $preflight = Invoke-ExactPost "/v1/feature-conveyor/repository-preflight" $preflightRequest
    $fingerprint = Assert-PreflightReceipt $preflight $plan $initialPauseRevision
    [void](Assert-RepositoryEligible ([string]$plan.repository_path) ([string]$plan.head_commit))
    $finalSet = Get-GrantSet ([string]$plan.repository_id)
    Assert-GrantSetPauseEpoch $finalSet $initialPauseRevision
    if ((Get-GrantState $finalSet $plan) -cne "exact_revision_1") {
        throw "Repository authority drifted after preflight."
    }
    $receipt = [ordered]@{
        schema_version = 1; status = $receiptStatus; repository_id = [string]$plan.repository_id
        registration_grant_revision = 1; cloud_disclosure_grant_revision = 1; autonomous_publication_grant_revision = 1
        base_branch = $baseBranch; head_commit = [string]$plan.head_commit; scope_sha256 = [string]$plan.scope_sha256
        approval_plan_sha256 = [string]$plan.approval_plan_sha256; preflight_fingerprint_sha256 = $fingerprint
    }
    Assert-AuthoringReceipt $receipt $plan
    $receiptLine = Get-CanonicalAuthoringReceiptLine $receipt
    if ($receiptLine -match '(?i)repository_path|[A-Z]:\\|\\\\') { throw "The authoring receipt was not path-free." }
    Write-PrivateFileAtomically $loaded.Paths.Receipt $receiptLine
    $receiptLine
} finally { Close-OwnerLoopback }

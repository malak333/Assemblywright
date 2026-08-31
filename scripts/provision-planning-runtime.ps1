[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$DataDir,
    [Parameter(Mandatory = $true)][string]$MasterExe,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$MasterExeSha256,
    [Parameter(Mandatory = $true)][string]$ProviderExe,
    [Parameter(Mandatory = $true)][string]$CodexExe,
    [Parameter(Mandatory = $true)][string]$OutputSchema,
    [Parameter(Mandatory = $true)][string]$GhExe,
    [Parameter(Mandatory = $true)][string]$CodexHome,
    [Parameter(Mandatory = $true)][string]$GhConfigDir,
    [Parameter(Mandatory = $true)][ValidatePattern('^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37}[A-Za-z0-9])?$')][string]$GithubOwner,
    [ValidatePattern('^[A-Za-z0-9_.-]{1,128}$')][string]$ServiceName = 'AssemblywrightMaster',
    [switch]$Confirm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $Confirm) { throw 'Provisioning requires -Confirm.' }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$ownerSid = $identity.User
if ($null -eq $ownerSid) { throw 'Provisioning owner does not have a concrete Windows SID.' }
$systemSid = [Security.Principal.SecurityIdentifier]::new([Security.Principal.WellKnownSidType]::LocalSystemSid,$null)
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Provisioning requires an elevated owner PowerShell.'
}
$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -eq $service) { throw 'The Assemblywright service must already be installed.' }
if ($service.Status -ne 'Stopped') {
    throw 'The Assemblywright service must be stopped before provisioning.'
}

if (-not ('AssemblywrightPlanningPathProofV9' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Win32.SafeHandles;
public static class AssemblywrightPlanningProfilesV9 {
  public const int ContractVersion=10;
  [DllImport("userenv.dll", CharSet=CharSet.Unicode)] public static extern int CreateAppContainerProfile(string n,string d,string x,IntPtr c,uint z,out IntPtr s);
  [DllImport("userenv.dll", CharSet=CharSet.Unicode)] public static extern int DeriveAppContainerSidFromAppContainerName(string n,out IntPtr s);
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern bool ConvertSidToStringSid(IntPtr s,out IntPtr p);
  [DllImport("advapi32.dll")] public static extern IntPtr FreeSid(IntPtr s);
  [DllImport("kernel32.dll")] public static extern IntPtr LocalFree(IntPtr p);
  public static string Ensure(string name) {
    IntPtr sid; int hr=CreateAppContainerProfile(name,name,name,IntPtr.Zero,0,out sid);
    if (hr < 0 && unchecked((uint)hr) != 0x800700B7U) Marshal.ThrowExceptionForHR(hr);
    if (hr < 0) { hr=DeriveAppContainerSidFromAppContainerName(name,out sid); if (hr < 0) Marshal.ThrowExceptionForHR(hr); }
    IntPtr text; if (!ConvertSidToStringSid(sid,out text)) throw new System.ComponentModel.Win32Exception();
    string value=Marshal.PtrToStringUni(text); LocalFree(text); FreeSid(sid); return value;
  }
}
public sealed class AssemblywrightPlanningFileProofV9 : IDisposable {
  internal readonly SafeFileHandle Handle;
  readonly FileStream stream;
  public readonly string CanonicalPath;
  public readonly string Identity;
  public readonly uint LinkCount;
  internal AssemblywrightPlanningFileProofV9(SafeFileHandle h,string p,string identity,uint links) {
    Handle=h; CanonicalPath=p; Identity=identity; LinkCount=links; stream=new FileStream(h,FileAccess.Read,4096,false);
  }
  public string Sha256() {
    stream.Position=0; using (SHA256 hash=SHA256.Create()) { return Hex(hash.ComputeHash(stream)); }
  }
  public string PrefixHex(int count) {
    byte[] bytes=new byte[count]; stream.Position=0; int total=0, read;
    while (total<count && (read=stream.Read(bytes,total,count-total))>0) total+=read;
    if (total!=count) throw new InvalidDataException();
    return Hex(bytes);
  }
  public void CopyNew(string destination) {
    stream.Position=0;
    using (FileStream output=new FileStream(destination,FileMode.CreateNew,FileAccess.Write,FileShare.None)) {
      stream.CopyTo(output); output.Flush(true);
    }
  }
  public void ApplyProtectedAcl(byte[] descriptor) { AssemblywrightPlanningPathProofV9.ApplyProtectedAcl(Handle,descriptor); }
  static string Hex(byte[] bytes) { StringBuilder text=new StringBuilder(bytes.Length*2); foreach(byte b in bytes) text.Append(b.ToString("x2")); return text.ToString(); }
  public void Dispose() { stream.Dispose(); }
}
public sealed class AssemblywrightPlanningPathGuardV9 : IDisposable {
  internal readonly SafeFileHandle Handle;
  public readonly string CanonicalPath;
  public readonly string Identity;
  internal AssemblywrightPlanningPathGuardV9(SafeFileHandle h,string p,string identity) { Handle=h; CanonicalPath=p; Identity=identity; }
  public void ApplyProtectedAcl(byte[] descriptor) { AssemblywrightPlanningPathProofV9.ApplyProtectedAcl(Handle,descriptor); }
  public void MergeProfileTraverse(string providerSid,string githubSid) { AssemblywrightPlanningPathProofV9.MergeProfileTraverse(Handle,providerSid,githubSid); }
  public void Dispose() { Handle.Dispose(); }
}
public static class AssemblywrightPlanningPathProofV9 {
  public const int ContractVersion=10;
  const uint GENERIC_READ=0x80000000, FILE_READ_ATTRIBUTES=0x80, READ_CONTROL=0x00020000, WRITE_DAC=0x00040000, WRITE_OWNER=0x00080000;
  const uint FILE_SHARE_READ=1, FILE_SHARE_WRITE=2, OPEN_EXISTING=3, FILE_ATTRIBUTE_REPARSE_POINT=0x400;
  const uint FILE_FLAG_BACKUP_SEMANTICS=0x02000000, FILE_FLAG_OPEN_REPARSE_POINT=0x00200000;
  [StructLayout(LayoutKind.Sequential)] struct FILETIME { public uint low, high; }
  [StructLayout(LayoutKind.Sequential)] struct BY_HANDLE_FILE_INFORMATION {
    public uint attributes; public FILETIME creation,access,write; public uint volume,sizeHigh,sizeLow,links,indexHigh,indexLow;
  }
  [StructLayout(LayoutKind.Sequential)] struct TRUSTEE_W {
    public IntPtr MultipleTrustee; public int MultipleTrusteeOperation,TrusteeForm,TrusteeType; public IntPtr Name;
  }
  [StructLayout(LayoutKind.Sequential)] struct EXPLICIT_ACCESS_W {
    public uint AccessPermissions; public int AccessMode; public uint Inheritance; public TRUSTEE_W Trustee;
  }
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  static extern SafeFileHandle CreateFile(string n,uint a,uint s,IntPtr p,uint d,uint f,IntPtr t);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  static extern uint GetFinalPathNameByHandle(SafeFileHandle h,StringBuilder p,uint z,uint f);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetFileInformationByHandle(SafeFileHandle h,out BY_HANDLE_FILE_INFORMATION i);
  [DllImport("ntdll.dll")] static extern int NtSetSecurityObject(SafeFileHandle h,uint i,IntPtr d);
  [DllImport("ntdll.dll")] static extern uint RtlNtStatusToDosError(int s);
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool IsValidSecurityDescriptor(IntPtr d);
  [DllImport("advapi32.dll", SetLastError=true)] static extern bool GetSecurityDescriptorControl(IntPtr d,out ushort c,out uint r);
  [DllImport("advapi32.dll")] static extern uint GetSecurityInfo(SafeFileHandle h,uint o,uint i,out IntPtr owner,out IntPtr group,out IntPtr dacl,out IntPtr sacl,out IntPtr descriptor);
  [DllImport("advapi32.dll")] static extern uint SetSecurityInfo(SafeFileHandle h,uint o,uint i,IntPtr owner,IntPtr group,IntPtr dacl,IntPtr sacl);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode)] static extern uint SetEntriesInAclW(uint count,[In] EXPLICIT_ACCESS_W[] entries,IntPtr oldAcl,out IntPtr newAcl);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertStringSidToSidW(string text,out IntPtr sid);
  [DllImport("shell32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern IntPtr CommandLineToArgvW(string c,out int n);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr p);
  static SafeFileHandle Open(string path,uint access,uint share,uint flags) {
    SafeFileHandle handle=CreateFile(path,access,share,IntPtr.Zero,OPEN_EXISTING,flags,IntPtr.Zero);
    if (handle.IsInvalid) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    return handle;
  }
  static string Final(SafeFileHandle handle) {
    StringBuilder value=new StringBuilder(32768); uint count=GetFinalPathNameByHandle(handle,value,(uint)value.Capacity,0);
    if (count==0 || count>=(uint)value.Capacity) throw new Win32Exception(Marshal.GetLastWin32Error());
    return value.ToString();
  }
  static string Identity(BY_HANDLE_FILE_INFORMATION info) { return info.volume.ToString("x8")+":"+info.indexHigh.ToString("x8")+info.indexLow.ToString("x8"); }
  public static AssemblywrightPlanningFileProofV9 OpenSourceFile(string path) {
    SafeFileHandle handle=Open(path,GENERIC_READ,0,FILE_FLAG_OPEN_REPARSE_POINT); BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(handle,out info)) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    if ((info.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0) { handle.Dispose(); throw new InvalidDataException(); }
    return new AssemblywrightPlanningFileProofV9(handle,Final(handle),Identity(info),info.links);
  }
  public static AssemblywrightPlanningFileProofV9 OpenTargetFile(string path) {
    SafeFileHandle handle=Open(path,GENERIC_READ|WRITE_DAC|WRITE_OWNER,FILE_SHARE_READ,FILE_FLAG_OPEN_REPARSE_POINT); BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(handle,out info)) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    if ((info.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0) { handle.Dispose(); throw new InvalidDataException(); }
    return new AssemblywrightPlanningFileProofV9(handle,Final(handle),Identity(info),info.links);
  }
  public static AssemblywrightPlanningFileProofV9 OpenExecutableGuard(string path) {
    SafeFileHandle handle=Open(path,GENERIC_READ,FILE_SHARE_READ,FILE_FLAG_OPEN_REPARSE_POINT); BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(handle,out info)) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    if ((info.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0) { handle.Dispose(); throw new InvalidDataException(); }
    return new AssemblywrightPlanningFileProofV9(handle,Final(handle),Identity(info),info.links);
  }
  public static AssemblywrightPlanningPathGuardV9 OpenSourceDirectory(string path) {
    SafeFileHandle handle=Open(path,FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT);
    BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(handle,out info)) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    if ((info.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0) { handle.Dispose(); throw new InvalidDataException(); }
    return new AssemblywrightPlanningPathGuardV9(handle,Final(handle),Identity(info));
  }
  public static AssemblywrightPlanningPathGuardV9 OpenTargetDirectory(string path) {
    SafeFileHandle handle=Open(path,FILE_READ_ATTRIBUTES|READ_CONTROL|WRITE_DAC|WRITE_OWNER,FILE_SHARE_READ|FILE_SHARE_WRITE,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT);
    BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(handle,out info)) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    if ((info.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0) { handle.Dispose(); throw new InvalidDataException(); }
    return new AssemblywrightPlanningPathGuardV9(handle,Final(handle),Identity(info));
  }
  public static AssemblywrightPlanningPathGuardV9 OpenSharedAclDirectory(string path) {
    SafeFileHandle handle=Open(path,FILE_READ_ATTRIBUTES|READ_CONTROL|WRITE_DAC,FILE_SHARE_READ|FILE_SHARE_WRITE,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT);
    BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(handle,out info)) { int error=Marshal.GetLastWin32Error(); handle.Dispose(); throw new Win32Exception(error); }
    if ((info.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0) { handle.Dispose(); throw new InvalidDataException(); }
    return new AssemblywrightPlanningPathGuardV9(handle,Final(handle),Identity(info));
  }
  public static string Canonical(string path) { using (AssemblywrightPlanningPathGuardV9 guard=OpenSourceDirectory(path)) return guard.CanonicalPath; }
  public static void ApplyProtectedAcl(SafeFileHandle handle,byte[] descriptor) {
    if (descriptor==null || descriptor.Length==0) throw new ArgumentException();
    GCHandle pinned=GCHandle.Alloc(descriptor,GCHandleType.Pinned);
    try {
      IntPtr security=pinned.AddrOfPinnedObject(); ushort control; uint revision;
      if (!IsValidSecurityDescriptor(security)) throw new InvalidDataException();
      if (!GetSecurityDescriptorControl(security,out control,out revision)) throw new Win32Exception(Marshal.GetLastWin32Error());
      const ushort SE_DACL_PROTECTED=0x1000,SE_SELF_RELATIVE=0x8000;
      if ((control&SE_DACL_PROTECTED)==0 || (control&SE_SELF_RELATIVE)==0) throw new InvalidDataException();
      const uint OWNER_SECURITY_INFORMATION=1,DACL_SECURITY_INFORMATION=4,PROTECTED_DACL_SECURITY_INFORMATION=0x80000000;
      int status=NtSetSecurityObject(handle,OWNER_SECURITY_INFORMATION|DACL_SECURITY_INFORMATION|PROTECTED_DACL_SECURITY_INFORMATION,security);
      if (status<0) throw new Win32Exception((int)RtlNtStatusToDosError(status));
    } finally { pinned.Free(); }
  }
  public static void MergeProfileTraverse(SafeFileHandle handle,string providerSid,string githubSid) {
    const uint SE_FILE_OBJECT=1,DACL_SECURITY_INFORMATION=4,FILE_TRAVERSE=0x20;
    const int SET_ACCESS=2,NO_MULTIPLE_TRUSTEE=0,TRUSTEE_IS_SID=0,TRUSTEE_IS_UNKNOWN=0;
    IntPtr owner=IntPtr.Zero,group=IntPtr.Zero,oldAcl=IntPtr.Zero,sacl=IntPtr.Zero,descriptor=IntPtr.Zero,newAcl=IntPtr.Zero,provider=IntPtr.Zero,github=IntPtr.Zero;
    try {
      uint status=GetSecurityInfo(handle,SE_FILE_OBJECT,DACL_SECURITY_INFORMATION,out owner,out group,out oldAcl,out sacl,out descriptor);
      if(status!=0)throw new Win32Exception((int)status);if(descriptor==IntPtr.Zero||oldAcl==IntPtr.Zero)throw new InvalidDataException();
      if(!ConvertStringSidToSidW(providerSid,out provider))throw new Win32Exception(Marshal.GetLastWin32Error());
      if(!ConvertStringSidToSidW(githubSid,out github))throw new Win32Exception(Marshal.GetLastWin32Error());
      EXPLICIT_ACCESS_W[] entries=new EXPLICIT_ACCESS_W[2]; IntPtr[] sids=new IntPtr[]{provider,github};
      for(int index=0;index<entries.Length;index++)entries[index]=new EXPLICIT_ACCESS_W { AccessPermissions=FILE_TRAVERSE,AccessMode=SET_ACCESS,Inheritance=0,Trustee=new TRUSTEE_W { MultipleTrustee=IntPtr.Zero,MultipleTrusteeOperation=NO_MULTIPLE_TRUSTEE,TrusteeForm=TRUSTEE_IS_SID,TrusteeType=TRUSTEE_IS_UNKNOWN,Name=sids[index] } };
      status=SetEntriesInAclW((uint)entries.Length,entries,oldAcl,out newAcl);if(status!=0||newAcl==IntPtr.Zero)throw new Win32Exception((int)status);
      status=SetSecurityInfo(handle,SE_FILE_OBJECT,DACL_SECURITY_INFORMATION,IntPtr.Zero,IntPtr.Zero,newAcl,IntPtr.Zero);if(status!=0)throw new Win32Exception((int)status);
    } finally { if(newAcl!=IntPtr.Zero)LocalFree(newAcl);if(provider!=IntPtr.Zero)LocalFree(provider);if(github!=IntPtr.Zero)LocalFree(github);if(descriptor!=IntPtr.Zero)LocalFree(descriptor); }
  }
  public static string[] ParseCommandLine(string command) {
    int count; IntPtr values=CommandLineToArgvW(command,out count); if (values==IntPtr.Zero || count<1) throw new Win32Exception();
    try { string[] result=new string[count]; for(int i=0;i<count;i++) result[i]=Marshal.PtrToStringUni(Marshal.ReadIntPtr(values,i*IntPtr.Size)); return result; }
    finally { LocalFree(values); }
  }
}
'@
}
if ([AssemblywrightPlanningPathProofV9]::ContractVersion -ne 10 -or [AssemblywrightPlanningProfilesV9]::ContractVersion -ne 10) {
    throw 'The loaded planning provisioning proof contract has the wrong version.'
}

$reparsePoint = [IO.FileAttributes]::ReparsePoint
$heldProofs = New-Object 'System.Collections.Generic.List[System.IDisposable]'
$sourceFileProofs = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
$targetFileProofs = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
$sourceDirectoryProofs = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
$targetDirectoryProofs = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
function Normalize-FinalValue([string]$Path) { ($Path -replace '^\\\\\?\\','').TrimEnd('\') }
function Normalize-LexicalPath([string]$Path) { Normalize-FinalValue ([IO.Path]::GetFullPath($Path)) }
function Assert-LocalDrivePath([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    $rootPath = [IO.Path]::GetPathRoot($fullPath)
    if ($rootPath -notmatch '^[A-Za-z]:\\$') { throw 'Provisioning paths must use a local drive.' }
}
function Assert-NoReparseAncestry([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    $current = if ($item.PSIsContainer) { $item } else { $item.Directory }
    $directories = @()
    while ($null -ne $current) {
        if (($current.Attributes -band $reparsePoint) -ne 0) { throw 'A provisioning path has reparse-point ancestry.' }
        $directories = @($current.FullName) + $directories
        $current = $current.Parent
    }
    $parentCanonical = $null
    foreach ($directory in $directories) {
        $proof = Open-ProvenDirectory $directory $false
        $canonical = Normalize-FinalValue $proof.CanonicalPath
        if ($null -ne $parentCanonical -and
            -not [IO.Path]::GetDirectoryName($canonical).TrimEnd('\').Equals($parentCanonical,[StringComparison]::OrdinalIgnoreCase)) {
            throw 'A provisioning ancestry entry changed its held parent identity.'
        }
        $parentCanonical = $canonical
    }
}
function Open-ProvenDirectory([string]$Path, [bool]$Target) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    $proofs = if ($Target) { $targetDirectoryProofs } else { $sourceDirectoryProofs }
    if ($proofs.ContainsKey($fullPath)) { return $proofs[$fullPath] }
    $proof = if ($Target) { [AssemblywrightPlanningPathProofV9]::OpenTargetDirectory($fullPath) } else { [AssemblywrightPlanningPathProofV9]::OpenSourceDirectory($fullPath) }
    [void]$heldProofs.Add($proof)
    $proofs[$fullPath] = $proof
    return $proof
}
function Open-SourceFileProof([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($sourceFileProofs.ContainsKey($fullPath)) { return $sourceFileProofs[$fullPath] }
    Assert-NoReparseAncestry $fullPath
    $proof = [AssemblywrightPlanningPathProofV9]::OpenSourceFile($fullPath)
    if ($proof.LinkCount -ne 1) { $proof.Dispose(); throw 'Every provisioning source file must have exactly one hard link.' }
    [void]$heldProofs.Add($proof)
    $sourceFileProofs[$fullPath] = $proof
    return $proof
}
function Open-TargetFileProof([string]$Path) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($targetFileProofs.ContainsKey($fullPath)) { return $targetFileProofs[$fullPath] }
    $proof = [AssemblywrightPlanningPathProofV9]::OpenTargetFile($fullPath)
    if ($proof.LinkCount -ne 1) { $proof.Dispose(); throw 'Every existing target file must have exactly one hard link.' }
    [void]$heldProofs.Add($proof)
    $targetFileProofs[$fullPath] = $proof
    return $proof
}
function Get-ProvenTreeManifest([string]$Path, [bool]$Source) {
    Assert-LocalDrivePath $Path
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw 'A required provisioning source or data root is absent.' }
    Assert-NoReparseAncestry $Path
    $root = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    $rootProof = Open-ProvenDirectory $root (-not $Source)
    $rootCanonical = Normalize-FinalValue $rootProof.CanonicalPath
    $entries = @()
    $queue = New-Object 'System.Collections.Generic.Queue[object]'
    $queue.Enqueue([PSCustomObject]@{ Path=$root; Relative=''; Canonical=$rootCanonical; Proof=$rootProof })
    while ($queue.Count -gt 0) {
        $parent = $queue.Dequeue()
        $children = @(Get-ChildItem -LiteralPath $parent.Path -Force)
        $exactNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        $foldedNames = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($child in $children) {
            if (-not $exactNames.Add($child.Name) -or -not $foldedNames.Add($child.Name)) {
                throw 'A provisioning directory contains duplicate or case-variant entry names.'
            }
        }
        if ($children.Count -ne $exactNames.Count -or $children.Count -ne $foldedNames.Count) {
            throw 'A provisioning directory entry count collapsed during validation.'
        }
        $children | Sort-Object Name | ForEach-Object {
            if (($_.Attributes -band $reparsePoint) -ne 0) { throw 'A provisioning tree contains a reparse point.' }
            $fullPath = [IO.Path]::GetFullPath($_.FullName)
            $relative = if ([string]::IsNullOrEmpty($parent.Relative)) { $_.Name } else { Join-Path $parent.Relative $_.Name }
            if ($_.PSIsContainer) {
                $proof = Open-ProvenDirectory $fullPath (-not $Source)
            } else {
                $proof = if ($Source) { Open-SourceFileProof $fullPath } else { Open-TargetFileProof $fullPath }
            }
            $canonical = Normalize-FinalValue $proof.CanonicalPath
            $canonicalParent = [IO.Path]::GetDirectoryName($canonical).TrimEnd('\')
            if (-not $canonicalParent.Equals($parent.Canonical, [StringComparison]::OrdinalIgnoreCase) -or
                -not (Test-PathWithin $canonical $rootCanonical)) {
                throw 'A held provisioning entry escaped or changed its held parent identity.'
            }
            $entries += [PSCustomObject]@{ Relative=$relative; Directory=$_.PSIsContainer; Proof=$proof; Canonical=$canonical; ParentIdentity=$parent.Proof.Identity }
            if ($_.PSIsContainer) {
                $queue.Enqueue([PSCustomObject]@{ Path=$fullPath; Relative=$relative; Canonical=$canonical; Proof=$proof })
            }
        }
    }
    return [PSCustomObject]@{ Root=$root; Canonical=$rootCanonical; RootIdentity=$rootProof.Identity; RootProof=$rootProof; Entries=$entries }
}
function Test-PathWithin([string]$Candidate, [string]$Root) {
    $candidatePath = $Candidate.TrimEnd('\')
    $rootPath = $Root.TrimEnd('\')
    $candidatePath.Equals($rootPath, [StringComparison]::OrdinalIgnoreCase) -or
        $candidatePath.StartsWith($rootPath + '\', [StringComparison]::OrdinalIgnoreCase)
}
function Assert-TopLevelAllowlist([string]$Root, [string[]]$Allowed) {
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) { return }
    $observed = @(Get-ChildItem -LiteralPath $Root -Force | ForEach-Object { $_.Name })
    $allowedExact = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $allowedFolded = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($name in $Allowed) {
        if (-not $allowedExact.Add($name) -or -not $allowedFolded.Add($name)) { throw 'The immutable allowlist itself is ambiguous.' }
    }
    $observedExact = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $observedFolded = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($name in $observed) {
        if (-not $observedExact.Add($name) -or -not $observedFolded.Add($name) -or -not $allowedExact.Contains($name)) {
            throw 'An immutable planning root contains a non-allowlisted or case-variant entry.'
        }
    }
    if ($observed.Count -ne $observedExact.Count -or $observed.Count -ne $observedFolded.Count) {
        throw 'An immutable planning root entry count collapsed during validation.'
    }
}

try {
    $data = [IO.Path]::GetFullPath($DataDir)
    $lexicalVolumeRoot = [IO.Path]::GetPathRoot($data)
    if ($data.TrimEnd('\').Equals($lexicalVolumeRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'The planning data directory may not be a volume root.'
    }
    $data = $data.TrimEnd('\')
    $targetManifest = Get-ProvenTreeManifest $data $false
    $canonicalData = $targetManifest.Canonical
    $systemRoots = @($env:SystemRoot, $env:ProgramFiles, ${env:ProgramFiles(x86)}, $env:ProgramData) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and (Test-Path -LiteralPath $_ -PathType Container) } |
        ForEach-Object { Normalize-FinalValue ([AssemblywrightPlanningPathProofV9]::Canonical($_)) }
    foreach ($systemRoot in $systemRoots) {
        if ((Test-PathWithin $canonicalData $systemRoot) -or (Test-PathWithin $systemRoot $canonicalData)) {
            throw 'The planning data directory may not contain or be inside an operating-system or shared-program path.'
        }
    }
    $profileRoots = @((Join-Path $env:SystemDrive 'Users'), $env:PUBLIC, $env:USERPROFILE) |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and (Test-Path -LiteralPath $_ -PathType Container) } |
        ForEach-Object { Normalize-FinalValue ([AssemblywrightPlanningPathProofV9]::Canonical($_)) }
    foreach ($profileRoot in $profileRoots) {
        if (Test-PathWithin $profileRoot $canonicalData) { throw 'The planning data directory may not contain a Windows profile or shared-user root.' }
    }

    $masterProof = Open-SourceFileProof $MasterExe
    $providerProof = Open-SourceFileProof $ProviderExe
    $codexProof = Open-SourceFileProof $CodexExe
    $schemaProof = Open-SourceFileProof $OutputSchema
    $ghProof = Open-SourceFileProof $GhExe
    $codexManifest = Get-ProvenTreeManifest $CodexHome $true
    $ghManifest = Get-ProvenTreeManifest $GhConfigDir $true
    $hosts = [IO.Path]::GetFullPath((Join-Path $GhConfigDir 'hosts.yml'))
    if (-not $sourceFileProofs.ContainsKey($hosts)) { throw 'GitHub CLI authentication state is absent.' }
    $sourceRoots = @($masterProof.CanonicalPath, $providerProof.CanonicalPath, $codexProof.CanonicalPath, $schemaProof.CanonicalPath, $ghProof.CanonicalPath, $codexManifest.Canonical, $ghManifest.Canonical) |
        ForEach-Object { Normalize-FinalValue $_ }
    foreach ($canonicalSource in $sourceRoots) {
        if ((Test-PathWithin $canonicalSource $canonicalData) -or (Test-PathWithin $canonicalData $canonicalSource)) {
            throw 'Provisioning sources and the destination data tree must not overlap.'
        }
    }
    if ($masterProof.Sha256() -ne $MasterExeSha256.ToLowerInvariant()) { throw 'The staged master executable does not match the owner-supplied release digest.' }

    foreach ($marker in @('master.sqlite3','master.owner.lock','development.token')) {
        if (-not $targetFileProofs.ContainsKey([IO.Path]::GetFullPath((Join-Path $data $marker)))) {
            throw 'The service-bound data directory is not an initialized Assemblywright master.'
        }
    }
    $databaseProof = $targetFileProofs[[IO.Path]::GetFullPath((Join-Path $data 'master.sqlite3'))]
    if ($databaseProof.PrefixHex(16) -ne '53514c69746520666f726d6174203300') { throw 'The Assemblywright master database marker is invalid.' }

    $serviceConfig = @(Get-CimInstance -ClassName Win32_Service | Where-Object { $_.Name -ceq $ServiceName })
    if ($serviceConfig.Count -ne 1 -or $serviceConfig[0].State -ne 'Stopped') { throw 'The exact Assemblywright service configuration is unavailable or active.' }
    $serviceArguments = [AssemblywrightPlanningPathProofV9]::ParseCommandLine($serviceConfig[0].PathName)
    if ((Normalize-LexicalPath $serviceArguments[0]) -ne (Normalize-FinalValue $masterProof.CanonicalPath)) { throw 'The service executable does not match the held release executable.' }
    $dataIndexes = @(for ($index=0; $index -lt $serviceArguments.Length; $index++) { if ($serviceArguments[$index] -ceq '--data-dir') { $index } })
    if ($dataIndexes.Count -ne 1 -or $dataIndexes[0] + 1 -ge $serviceArguments.Length -or (@($serviceArguments | Where-Object { $_ -ceq 'service-run' })).Count -ne 1) {
        throw 'The service command line does not contain one exact Assemblywright data binding.'
    }
    $serviceData = Normalize-FinalValue ([AssemblywrightPlanningPathProofV9]::Canonical($serviceArguments[$dataIndexes[0] + 1]))
    if (-not $serviceData.Equals($canonicalData, [StringComparison]::OrdinalIgnoreCase)) { throw 'The requested data directory is not the installed service data directory.' }

    $providerName = 'Assemblywright.Planning.Provider.v1'
    $githubName = 'Assemblywright.Planning.Github.v1'
    $locator = Join-Path $data 'planning-runtime'
    $programDataPath = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
    if ([string]::IsNullOrWhiteSpace($programDataPath)) { throw 'The canonical common application data root is unavailable.' }
    $programDataProof = [AssemblywrightPlanningPathProofV9]::OpenSharedAclDirectory([IO.Path]::GetFullPath($programDataPath))
    [void]$heldProofs.Add($programDataProof)
    $canonicalProgramData = Normalize-FinalValue $programDataProof.CanonicalPath
    $runtimeVendor = Join-Path $canonicalProgramData 'Assemblywright'
    $runtimeNamespace = Join-Path $runtimeVendor 'planning-runtime'
    $planning = Join-Path $runtimeNamespace $ServiceName
    $provider = Join-Path $planning 'provider'
    $github = Join-Path $planning 'github'
    $masterCheck = Join-Path $planning 'master-check'
    $providerCodexHome = Join-Path $provider 'codex-home'
    $providerReconciliation = Join-Path $provider 'reconciliation'
    $providerTemp = Join-Path $provider 'temp'
    $providerLocalAppData = Join-Path $provider 'local-app-data'
    $githubConfig = Join-Path $github 'gh-config'
    if (Test-Path -LiteralPath $planning) {
        [void](Get-ProvenTreeManifest $planning $false)
        Assert-TopLevelAllowlist $planning @('provider','github','master-check')
    }
    if (Test-Path -LiteralPath $provider) { Assert-TopLevelAllowlist $provider @('brainstorming-provider.exe','codex.exe','brainstorming-output-schema.json','runtime.json','codex-home','reconciliation','temp','local-app-data') }
    if (Test-Path -LiteralPath $github) { Assert-TopLevelAllowlist $github @('gh.exe','gh-config') }
    if (Test-Path -LiteralPath $masterCheck) { Assert-TopLevelAllowlist $masterCheck @('assemblywright-master.exe') }

    $providerSid = [AssemblywrightPlanningProfilesV9]::Ensure($providerName)
    $githubSid = [AssemblywrightPlanningProfilesV9]::Ensure($githubName)
    if ($providerSid -eq $githubSid) { throw 'Planning profile identities are not distinct.' }
    $providerAclSid = [Security.Principal.SecurityIdentifier]::new($providerSid)
    $githubAclSid = [Security.Principal.SecurityIdentifier]::new($githubSid)
    if ($providerAclSid.Value -cne $providerSid -or $githubAclSid.Value -cne $githubSid) {
        throw 'Planning profile SID text is not exact canonical SID form.'
    }

function New-ExactAclBytes([bool]$Directory, [object[]]$Rules) {
    $acl = if ($Directory) { New-Object Security.AccessControl.DirectorySecurity } else { New-Object Security.AccessControl.FileSecurity }
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner($ownerSid)
    foreach ($rule in $Rules) {
        $inheritance = if ($Directory -and $rule.Inherit) { [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit' } else { [Security.AccessControl.InheritanceFlags]::None }
        if (-not ($rule.Identity -is [Security.Principal.SecurityIdentifier])) { throw 'ACL construction requires an exact SecurityIdentifier.' }
        $entry = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.IdentityReference]$rule.Identity, $rule.Rights, $inheritance, [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow)
        $acl.AddAccessRule($entry) | Out-Null
    }
    $acl.GetSecurityDescriptorBinaryForm()
}
function Set-HeldAcl([object]$Proof, [bool]$Directory, [object[]]$Rules, [string]$Role) {
    if ($null -eq $Proof) { throw 'ACL mutation requires a held manifest proof.' }
    [byte[]]$descriptor = New-ExactAclBytes $Directory $Rules
    try {
        $Proof.ApplyProtectedAcl($descriptor)
    } catch {
        $failure = $_.Exception
        while ($null -ne $failure.InnerException) { $failure = $failure.InnerException }
        $status = if ($failure -is [ComponentModel.Win32Exception]) { $failure.NativeErrorCode } else { $failure.HResult -band 0xffff }
        $kind = if ($Directory) { 'directory' } else { 'file' }
        throw "ACL mutation failed for held proof role=$Role kind=$kind status=$status."
    }
}
function Ensure-ProvenTargetDirectory([string]$Path, [object]$ParentProof) {
    $fullPath = [IO.Path]::GetFullPath($Path)
    if (Test-Path -LiteralPath $fullPath) {
        if (-not (Test-Path -LiteralPath $fullPath -PathType Container)) { throw 'A planning directory destination has the wrong type.' }
    } else {
        New-Item -ItemType Directory -Path $fullPath | Out-Null
    }
    $proof = Open-ProvenDirectory $fullPath $true
    $canonical = Normalize-FinalValue $proof.CanonicalPath
    $canonicalParent = [IO.Path]::GetDirectoryName($canonical).TrimEnd('\')
    $heldParent = (Normalize-FinalValue $ParentProof.CanonicalPath).TrimEnd('\')
    if (-not $canonicalParent.Equals($heldParent,[StringComparison]::OrdinalIgnoreCase)) {
        throw 'A newly created planning directory escaped or changed its held parent identity.'
    }
    return $proof
}
function MasterRules([bool]$inherit) {
    @(
        @{ Identity=$ownerSid; Rights=[Security.AccessControl.FileSystemRights]::FullControl; Inherit=$inherit },
        @{ Identity=$systemSid; Rights=[Security.AccessControl.FileSystemRights]::FullControl; Inherit=$inherit }
    )
}
function ScopeRules([Security.Principal.SecurityIdentifier]$sid, [Security.AccessControl.FileSystemRights]$rights, [bool]$inherit) {
    $rules = MasterRules $inherit
    $rules += @{ Identity=$sid; Rights=$rights; Inherit=$inherit }
    $rules
}
function Set-ManifestDescendantsAcl([object]$Manifest, [object[]]$Rules, [string]$Role) {
    $ordinal = 0
    foreach ($entry in @($Manifest.Entries | Sort-Object { $_.Relative.Length } -Descending)) {
        Set-HeldAcl $entry.Proof $entry.Directory $Rules "$Role-descendant-$ordinal"
        $ordinal++
    }
}
function Set-ProtectedManifestAcl([object]$Manifest, [Security.Principal.SecurityIdentifier]$sid, [Security.AccessControl.FileSystemRights]$rights, [bool]$InheritNewChildren, [string]$Role) {
    $ordinal = 0
    foreach ($entry in @($Manifest.Entries | Sort-Object { $_.Relative.Length } -Descending)) {
        # A pre-existing writable directory must remain an atomic inheritance boundary after it is
        # protected. Files never carry inheritance flags. Both cases use only the same exact SIDs.
        $entryRules = ScopeRules $sid $rights ($InheritNewChildren -and $entry.Directory)
        Set-HeldAcl $entry.Proof $entry.Directory $entryRules "$Role-descendant-$ordinal"
        $ordinal++
    }
    # Writable roots carry only the same exact owner/SYSTEM/profile ACEs, marked inheritable.
    # Windows therefore installs an exact child DACL atomically at CreateFile/CreateDirectory time;
    # no broad token-default DACL is observable before a later repair.
    $rootRules = ScopeRules $sid $rights $InheritNewChildren
    Set-HeldAcl $Manifest.RootProof $true $rootRules "$Role-root"
}
function Assert-ManifestUnchanged([object]$Expected) {
    $fresh = Get-ProvenTreeManifest $Expected.Root $false
    if ($fresh.RootIdentity -cne $Expected.RootIdentity -or $fresh.Entries.Count -ne $Expected.Entries.Count) {
        throw 'A protected planning manifest changed after its held ACL mutation.'
    }
    $freshByRelative = [Collections.Generic.Dictionary[string,object]]::new([StringComparer]::Ordinal)
    foreach ($entry in $fresh.Entries) {
        if ($freshByRelative.ContainsKey($entry.Relative)) { throw 'A protected planning manifest contains a duplicate relative path.' }
        $freshByRelative.Add($entry.Relative,$entry)
    }
    foreach ($entry in $Expected.Entries) {
        if (-not $freshByRelative.ContainsKey($entry.Relative)) { throw 'A protected planning manifest lost or renamed an entry.' }
        $observed = $freshByRelative[$entry.Relative]
        if ($observed.Directory -ne $entry.Directory -or $observed.Proof.Identity -cne $entry.Proof.Identity -or $observed.ParentIdentity -cne $entry.ParentIdentity) {
            throw 'A protected planning manifest entry changed identity.'
        }
    }
}
function Assert-ManifestTopLevelExact([object]$Manifest, [string[]]$Allowed) {
    $top = @($Manifest.Entries | Where-Object { [string]::IsNullOrEmpty([IO.Path]::GetDirectoryName($_.Relative)) } | ForEach-Object { $_.Relative })
    $exact = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $folded = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($name in $top) { if (-not $exact.Add($name) -or -not $folded.Add($name)) { throw 'A final immutable root contains case-variant names.' } }
    if ($top.Count -ne $Allowed.Count -or $top.Count -ne $exact.Count -or $top.Count -ne $folded.Count) { throw 'A final immutable root does not have the exact raw entry count.' }
    foreach ($name in $Allowed) { if (-not $exact.Contains($name)) { throw 'A final immutable root is missing an exact allowlisted entry.' } }
}
function Install-ProvenFile([AssemblywrightPlanningFileProofV9]$Proof, [string]$Destination) {
    $destinationPath = [IO.Path]::GetFullPath($Destination)
    if (Test-Path -LiteralPath $destinationPath) {
        if (-not (Test-Path -LiteralPath $destinationPath -PathType Leaf)) { throw 'An immutable planning file destination has the wrong type.' }
        $targetProof = Open-TargetFileProof $destinationPath
        if ($targetProof.Sha256() -ne $Proof.Sha256()) { throw 'An existing immutable planning file does not match the held source bytes.' }
        return
    }
    $Proof.CopyNew($destinationPath)
    $targetProof = Open-TargetFileProof $destinationPath
    if ($targetProof.Sha256() -ne $Proof.Sha256()) { throw 'A copied planning file does not match the held source bytes.' }
}
function Install-ProvenStateTree([object]$Manifest, [string]$Destination) {
    foreach ($entry in @($Manifest.Entries | Where-Object { $_.Directory } | Sort-Object { $_.Relative.Length })) {
        $target = Join-Path $Destination $entry.Relative
        $parentProof = Open-ProvenDirectory ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($target))) $true
        [void](Ensure-ProvenTargetDirectory $target $parentProof)
    }
    foreach ($entry in @($Manifest.Entries | Where-Object { -not $_.Directory })) {
        $target = Join-Path $Destination $entry.Relative
        if (Test-Path -LiteralPath $target) {
            if (-not (Test-Path -LiteralPath $target -PathType Leaf)) { throw 'A private state file has the wrong type.' }
            $existing = Open-TargetFileProof $target
            if ($existing.Sha256() -ne $entry.Proof.Sha256()) { throw 'Existing private state bytes do not match the held source proof.' }
        } else {
            Install-ProvenFile $entry.Proof $target
        }
    }
}
function Get-BytesSha256([byte[]]$Bytes) {
    $hasher = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($hasher.ComputeHash($Bytes))).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
}

# Remove inherited broad access from all existing master state before creating or copying anything.
Set-ManifestDescendantsAcl $targetManifest (MasterRules $false) 'initial-master'
Set-HeldAcl $targetManifest.RootProof $true (MasterRules $true) 'initial-master-root'

$locatorProof = Ensure-ProvenTargetDirectory $locator $targetManifest.RootProof
$runtimeVendorProof = Ensure-ProvenTargetDirectory $runtimeVendor $programDataProof
$runtimeNamespaceProof = Ensure-ProvenTargetDirectory $runtimeNamespace $runtimeVendorProof
$planningProof = Ensure-ProvenTargetDirectory $planning $runtimeNamespaceProof
$providerDirectoryProof = Ensure-ProvenTargetDirectory $provider $planningProof
$providerCodexHomeProof = Ensure-ProvenTargetDirectory $providerCodexHome $providerDirectoryProof
$providerReconciliationProof = Ensure-ProvenTargetDirectory $providerReconciliation $providerDirectoryProof
$providerTempProof = Ensure-ProvenTargetDirectory $providerTemp $providerDirectoryProof
$providerLocalAppDataProof = Ensure-ProvenTargetDirectory $providerLocalAppData $providerDirectoryProof
$githubDirectoryProof = Ensure-ProvenTargetDirectory $github $planningProof
$githubConfigProof = Ensure-ProvenTargetDirectory $githubConfig $githubDirectoryProof
$masterCheckProof = Ensure-ProvenTargetDirectory $masterCheck $planningProof
$traverse = [Security.AccessControl.FileSystemRights]::Traverse
$readExecute = [Security.AccessControl.FileSystemRights]::ReadAndExecute
$modify = [Security.AccessControl.FileSystemRights]::Modify
$planningRules = MasterRules $true
$planningRules += @{Identity=$providerAclSid;Rights=$traverse;Inherit=$false}, @{Identity=$githubAclSid;Rights=$traverse;Inherit=$false}
try {
    $programDataProof.MergeProfileTraverse($providerSid,$githubSid)
    $runtimeVendorProof.MergeProfileTraverse($providerSid,$githubSid)
    $runtimeNamespaceProof.MergeProfileTraverse($providerSid,$githubSid)
} catch {
    $failure = $_.Exception
    while ($null -ne $failure.InnerException) { $failure = $failure.InnerException }
    $status = if ($failure -is [ComponentModel.Win32Exception]) { $failure.NativeErrorCode } else { $failure.HResult -band 0xffff }
    throw "Shared planning ancestry ACL mutation failed status=$status."
}
Set-HeldAcl $targetManifest.RootProof $true (MasterRules $true) 'data-master-root'
Set-HeldAcl $locatorProof $true (MasterRules $true) 'planning-locator-root'
Set-HeldAcl $planningProof $true $planningRules 'planning-traverse-root'
# Inheritable scope ACLs protect newly copied children during provisioning. Every descendant is
# converted to an explicit protected ACL after copying, before validation can enable the runtime.
Set-HeldAcl $providerDirectoryProof $true (ScopeRules $providerAclSid $readExecute $true) 'provider-staging-root'
Set-HeldAcl $providerCodexHomeProof $true (ScopeRules $providerAclSid $modify $true) 'provider-codex-state-staging-root'
Set-HeldAcl $providerReconciliationProof $true (ScopeRules $providerAclSid $modify $true) 'provider-reconciliation-staging-root'
Set-HeldAcl $providerTempProof $true (ScopeRules $providerAclSid $modify $true) 'provider-temp-staging-root'
Set-HeldAcl $providerLocalAppDataProof $true (ScopeRules $providerAclSid $modify $true) 'provider-local-app-data-staging-root'
Set-HeldAcl $githubDirectoryProof $true (ScopeRules $githubAclSid $readExecute $true) 'github-staging-root'
Set-HeldAcl $githubConfigProof $true (ScopeRules $githubAclSid $modify $true) 'github-state-staging-root'
Set-HeldAcl $masterCheckProof $true (MasterRules $true) 'master-check-staging-root'

Install-ProvenFile $providerProof (Join-Path $provider 'brainstorming-provider.exe')
Install-ProvenFile $codexProof (Join-Path $provider 'codex.exe')
Install-ProvenFile $schemaProof (Join-Path $provider 'brainstorming-output-schema.json')
Install-ProvenStateTree $codexManifest $providerCodexHome
Install-ProvenFile $ghProof (Join-Path $github 'gh.exe')
Install-ProvenStateTree $ghManifest $githubConfig
Install-ProvenFile $masterProof (Join-Path $masterCheck 'assemblywright-master.exe')

$providerHash = $providerProof.Sha256()
$codexHash = $codexProof.Sha256()
$schemaHash = $schemaProof.Sha256()
$ghHash = $ghProof.Sha256()

$providerConfig = [ordered]@{schema_version=1;enabled=$true;catalog_revision=1;provider_id='openai.codex';model_id='gpt-5.6-sol';adapter_kind='codex_exec_v1';brainstorming_provider_sha256=$providerHash;codex_executable_sha256=$codexHash;output_schema_sha256=$schemaHash;gh_executable_sha256=$ghHash;github_owner=$GithubOwner}
$masterConfig = [ordered]@{schema_version=4;enabled=$true;catalog_revision=1;provider_id='openai.codex';model_id='gpt-5.6-sol';adapter_kind='codex_exec_v1';brainstorming_provider_sha256=$providerHash;codex_executable_sha256=$codexHash;output_schema_sha256=$schemaHash;gh_executable_sha256=$ghHash;github_owner=$GithubOwner;provider_profile_name=$providerName;provider_profile_sid=$providerSid;github_profile_name=$githubName;github_profile_sid=$githubSid;provisioning_owner_sid=$ownerSid.Value;runtime_instance=$ServiceName}

function Write-AtomicJson([string]$Path, [object]$Value) {
    $temporary = "$Path.new"
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Compress))
    $expectedHash = Get-BytesSha256 $bytes
    if (Test-Path -LiteralPath $Path) {
        $existing = Open-TargetFileProof $Path
        if ($existing.Sha256() -ne $expectedHash) { throw 'An existing immutable planning configuration does not match the exact provision request.' }
        return
    }
    $stream = New-Object IO.FileStream($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
    Move-Item -LiteralPath $temporary -Destination $Path
    $created = Open-TargetFileProof $Path
    if ($created.Sha256() -ne $expectedHash) { throw 'A persisted planning configuration did not retain the exact bytes.' }
}
Write-AtomicJson (Join-Path $provider 'runtime.json') $providerConfig
Write-AtomicJson (Join-Path $locator 'runtime-v4.json') $masterConfig

# Capture every final entry before ACL mutation. ACLs are applied only through these held handles;
# a later unmanifested entry is detected without receiving an ACL.
$finalDataManifest = Get-ProvenTreeManifest $data $false
$planningManifest = Get-ProvenTreeManifest $planning $false
$providerManifest = Get-ProvenTreeManifest $provider $false
$providerCodexManifest = Get-ProvenTreeManifest $providerCodexHome $false
$providerReconciliationManifest = Get-ProvenTreeManifest $providerReconciliation $false
$providerTempManifest = Get-ProvenTreeManifest $providerTemp $false
$providerLocalAppDataManifest = Get-ProvenTreeManifest $providerLocalAppData $false
$githubTargetManifest = Get-ProvenTreeManifest $github $false
$githubConfigManifest = Get-ProvenTreeManifest $githubConfig $false
$masterCheckManifest = Get-ProvenTreeManifest $masterCheck $false
Assert-ManifestTopLevelExact $planningManifest @('provider','github','master-check')
Assert-ManifestTopLevelExact $providerManifest @('brainstorming-provider.exe','codex.exe','brainstorming-output-schema.json','runtime.json','codex-home','reconciliation','temp','local-app-data')
Assert-ManifestTopLevelExact $githubTargetManifest @('gh.exe','gh-config')
Assert-ManifestTopLevelExact $masterCheckManifest @('assemblywright-master.exe')
Set-ProtectedManifestAcl $providerManifest $providerAclSid $readExecute $false 'provider-final'
Set-ProtectedManifestAcl $providerCodexManifest $providerAclSid $modify $true 'provider-codex-state-final'
Set-ProtectedManifestAcl $providerReconciliationManifest $providerAclSid $modify $true 'provider-reconciliation-final'
Set-ProtectedManifestAcl $providerTempManifest $providerAclSid $modify $true 'provider-temp-final'
Set-ProtectedManifestAcl $providerLocalAppDataManifest $providerAclSid $modify $true 'provider-local-app-data-final'
Set-ProtectedManifestAcl $githubTargetManifest $githubAclSid $readExecute $false 'github-final'
Set-ProtectedManifestAcl $githubConfigManifest $githubAclSid $modify $true 'github-state-final'
Set-ManifestDescendantsAcl $masterCheckManifest (MasterRules $false) 'master-check-final'
Set-HeldAcl $masterCheckManifest.RootProof $true (MasterRules $false) 'master-check-final-root'
Set-HeldAcl $planningManifest.RootProof $true $planningRules 'planning-final-root'
$locatorManifest = Get-ProvenTreeManifest $locator $false
$planningRuntimeEntry = @($locatorManifest.Entries | Where-Object { $_.Relative -ceq 'runtime-v4.json' })
if ($planningRuntimeEntry.Count -ne 1 -or $planningRuntimeEntry[0].Directory) { throw 'The master planning configuration is not exactly manifested.' }
Set-HeldAcl $planningRuntimeEntry[0].Proof $false (MasterRules $false) 'planning-runtime-config-final'
Set-HeldAcl $locatorManifest.RootProof $true (MasterRules $true) 'planning-locator-final-root'
foreach ($manifest in @($finalDataManifest,$locatorManifest,$planningManifest,$providerManifest,$providerCodexManifest,$providerReconciliationManifest,$providerTempManifest,$providerLocalAppDataManifest,$githubTargetManifest,$githubConfigManifest,$masterCheckManifest)) {
    Assert-ManifestUnchanged $manifest
}

$stagedMaster = Join-Path $masterCheck 'assemblywright-master.exe'
$stagedMasterGuard = [AssemblywrightPlanningPathProofV9]::OpenExecutableGuard($stagedMaster)
if ($stagedMasterGuard.LinkCount -ne 1 -or $stagedMasterGuard.Sha256() -ne $MasterExeSha256.ToLowerInvariant()) {
    $stagedMasterGuard.Dispose()
    throw 'The protected staged master executable identity drifted before validation.'
}
[void]$heldProofs.Add($stagedMasterGuard)
& $stagedMaster --data-dir $data planning-runtime-check --confirm
if ($LASTEXITCODE -ne 0) { throw 'Planning runtime validation failed closed.' }
Write-Output '{"status":"planning_runtime_provisioned","live_evidence_required":true}'
} finally {
    for ($index=$heldProofs.Count-1; $index -ge 0; $index--) { $heldProofs[$index].Dispose() }
}

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$DataDir,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$MasterExeSha256,
    [ValidatePattern('^[A-Za-z0-9_.-]{1,128}$')][string]$ServiceName = 'AssemblywrightMaster',
    [switch]$Confirm
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'Checking the private planning runtime requires -Confirm.' }
$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -eq $service -or $service.Status -ne 'Stopped') { throw 'The exact Assemblywright service must be installed and stopped.' }

if (-not ('AssemblywrightPlanningCheckProofV4' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Microsoft.Win32.SafeHandles;
public sealed class AssemblywrightPlanningCheckProofV4 : IDisposable {
  public const int ContractVersion=4;
  const uint GENERIC_READ=0x80000000, FILE_READ_ATTRIBUTES=0x80, FILE_SHARE_READ=1, FILE_SHARE_WRITE=2;
  const uint OPEN_EXISTING=3, FILE_ATTRIBUTE_REPARSE_POINT=0x400, FILE_FLAG_BACKUP_SEMANTICS=0x02000000, FILE_FLAG_OPEN_REPARSE_POINT=0x00200000;
  [StructLayout(LayoutKind.Sequential)] struct FILETIME { public uint low,high; }
  [StructLayout(LayoutKind.Sequential)] struct INFO { public uint attributes; public FILETIME creation,access,write; public uint volume,sizeHigh,sizeLow,links,indexHigh,indexLow; }
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFile(string n,uint a,uint s,IntPtr p,uint d,uint f,IntPtr t);
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern uint GetFinalPathNameByHandle(SafeFileHandle h,StringBuilder p,uint z,uint f);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetFileInformationByHandle(SafeFileHandle h,out INFO i);
  [DllImport("shell32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CommandLineToArgvW(string c,out int n);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr p);
  readonly SafeFileHandle handle; readonly FileStream stream;
  public readonly string CanonicalPath; public readonly uint LinkCount;
  AssemblywrightPlanningCheckProofV4(SafeFileHandle h,string p,uint links,bool file) { handle=h; CanonicalPath=p; LinkCount=links; if(file) stream=new FileStream(h,FileAccess.Read,4096,false); }
  static SafeFileHandle OpenHandle(string path,uint access,uint share,uint flags) { SafeFileHandle h=CreateFile(path,access,share,IntPtr.Zero,OPEN_EXISTING,flags,IntPtr.Zero); if(h.IsInvalid){int e=Marshal.GetLastWin32Error();h.Dispose();throw new Win32Exception(e);} return h; }
  static string Final(SafeFileHandle h) { StringBuilder p=new StringBuilder(32768); uint n=GetFinalPathNameByHandle(h,p,(uint)p.Capacity,0); if(n==0||n>=(uint)p.Capacity)throw new Win32Exception(); return p.ToString(); }
  static AssemblywrightPlanningCheckProofV4 Open(string path,uint access,uint share,uint flags,bool file) { SafeFileHandle h=OpenHandle(path,access,share,flags|FILE_FLAG_OPEN_REPARSE_POINT); INFO i; if(!GetFileInformationByHandle(h,out i)){int e=Marshal.GetLastWin32Error();h.Dispose();throw new Win32Exception(e);} if((i.attributes&FILE_ATTRIBUTE_REPARSE_POINT)!=0){h.Dispose();throw new InvalidDataException();} return new AssemblywrightPlanningCheckProofV4(h,Final(h),i.links,file); }
  public static AssemblywrightPlanningCheckProofV4 OpenServiceImage(string path) { return Open(path,GENERIC_READ,0,0,true); }
  public static AssemblywrightPlanningCheckProofV4 OpenStagedImage(string path) { return Open(path,GENERIC_READ,FILE_SHARE_READ,0,true); }
  public static AssemblywrightPlanningCheckProofV4 OpenDirectory(string path) { return Open(path,FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE,FILE_FLAG_BACKUP_SEMANTICS,false); }
  public string Sha256() { if(stream==null)throw new InvalidOperationException(); stream.Position=0; using(SHA256 h=SHA256.Create()){byte[] b=h.ComputeHash(stream);StringBuilder s=new StringBuilder(64);foreach(byte v in b)s.Append(v.ToString("x2"));return s.ToString();} }
  public string ReadUtf8(int maximum) { if(stream==null||stream.Length<=0||stream.Length>maximum)throw new InvalidDataException();byte[] b=new byte[stream.Length];stream.Position=0;int total=0,read;while(total<b.Length&&(read=stream.Read(b,total,b.Length-total))>0)total+=read;if(total!=b.Length)throw new EndOfStreamException();return new UTF8Encoding(false,true).GetString(b); }
  public static string[] ParseCommandLine(string command) { int count; IntPtr values=CommandLineToArgvW(command,out count); if(values==IntPtr.Zero||count<1)throw new Win32Exception(); try{string[] result=new string[count];for(int i=0;i<count;i++)result[i]=Marshal.PtrToStringUni(Marshal.ReadIntPtr(values,i*IntPtr.Size));return result;}finally{LocalFree(values);} }
  public void Dispose() { if(stream!=null)stream.Dispose();else handle.Dispose(); }
}
'@
}
if ([AssemblywrightPlanningCheckProofV4]::ContractVersion -ne 4) { throw 'The loaded planning check proof contract has the wrong version.' }

$held = New-Object 'System.Collections.Generic.List[System.IDisposable]'
$heldDirectories = @{}
function Normalize-Final([string]$Path) { ($Path -replace '^\\\\\?\\','').TrimEnd('\') }
function Hold-Ancestry([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    $current = if ($item.PSIsContainer) { $item } else { $item.Directory }
    $directories = @()
    while ($null -ne $current) { $directories = @($current.FullName) + $directories; $current = $current.Parent }
    $parentCanonical = $null
    foreach ($directory in $directories) {
        $fullPath = [IO.Path]::GetFullPath($directory)
        if ($heldDirectories.ContainsKey($fullPath)) { $guard = $heldDirectories[$fullPath] } else {
            $guard = [AssemblywrightPlanningCheckProofV4]::OpenDirectory($fullPath)
            [void]$held.Add($guard)
            $heldDirectories[$fullPath] = $guard
        }
        $canonical = Normalize-Final $guard.CanonicalPath
        if ($null -ne $parentCanonical -and -not [IO.Path]::GetDirectoryName($canonical).TrimEnd('\').Equals($parentCanonical,[StringComparison]::OrdinalIgnoreCase)) {
            throw 'A check path changed its held parent identity.'
        }
        $parentCanonical = $canonical
    }
}
try {
    $data = [IO.Path]::GetFullPath($DataDir).TrimEnd('\')
    Hold-Ancestry $data
    $dataGuard = [AssemblywrightPlanningCheckProofV4]::OpenDirectory($data)
    [void]$held.Add($dataGuard)
    $canonicalData = Normalize-Final $dataGuard.CanonicalPath
    $serviceConfig = @(Get-CimInstance -ClassName Win32_Service | Where-Object { $_.Name -ceq $ServiceName })
    if ($serviceConfig.Count -ne 1 -or $serviceConfig[0].State -ne 'Stopped') { throw 'The exact Assemblywright service configuration is unavailable or active.' }
    $arguments = [AssemblywrightPlanningCheckProofV4]::ParseCommandLine($serviceConfig[0].PathName)
    $dataIndexes = @(for ($index=0; $index -lt $arguments.Length; $index++) { if ($arguments[$index] -ceq '--data-dir') { $index } })
    if ($dataIndexes.Count -ne 1 -or $dataIndexes[0] + 1 -ge $arguments.Length -or (@($arguments | Where-Object { $_ -ceq 'service-run' })).Count -ne 1) { throw 'The service command line is not exactly data-bound.' }
    $serviceDataGuard = [AssemblywrightPlanningCheckProofV4]::OpenDirectory($arguments[$dataIndexes[0] + 1])
    [void]$held.Add($serviceDataGuard)
    if (-not (Normalize-Final $serviceDataGuard.CanonicalPath).Equals($canonicalData,[StringComparison]::OrdinalIgnoreCase)) { throw 'The requested data directory is not service-bound.' }
    Hold-Ancestry $arguments[0]
    $serviceImage = [AssemblywrightPlanningCheckProofV4]::OpenServiceImage($arguments[0])
    [void]$held.Add($serviceImage)
    if ($serviceImage.LinkCount -ne 1 -or $serviceImage.Sha256() -ne $MasterExeSha256.ToLowerInvariant()) { throw 'The installed service image does not match the expected release digest.' }
    $locator = Join-Path $data 'planning-runtime\runtime-v4.json'
    Hold-Ancestry $locator
    $locatorImage = [AssemblywrightPlanningCheckProofV4]::OpenStagedImage($locator)
    [void]$held.Add($locatorImage)
    if ($locatorImage.LinkCount -ne 1) { throw 'The planning runtime locator is not a single-link file.' }
    $locatorConfig = $locatorImage.ReadUtf8(16384) | ConvertFrom-Json
    if ($locatorConfig.schema_version -ne 4 -or $locatorConfig.runtime_instance -notmatch '^[A-Za-z0-9_.-]{1,128}$') { throw 'The planning runtime locator contract is invalid.' }
    $programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
    if ([string]::IsNullOrWhiteSpace($programData)) { throw 'The canonical common application data root is unavailable.' }
    $runtimeRoot = Join-Path (Join-Path (Join-Path $programData 'Assemblywright') 'planning-runtime') $locatorConfig.runtime_instance
    $staged = Join-Path $runtimeRoot 'master-check\assemblywright-master.exe'
    Hold-Ancestry $staged
    $stagedImage = [AssemblywrightPlanningCheckProofV4]::OpenStagedImage($staged)
    [void]$held.Add($stagedImage)
    if ($stagedImage.LinkCount -ne 1 -or $stagedImage.Sha256() -ne $MasterExeSha256.ToLowerInvariant()) { throw 'The protected staged master image does not match the expected release digest.' }
    & $staged --data-dir $data planning-runtime-check --confirm
    if ($LASTEXITCODE -ne 0) { throw 'Planning runtime is unavailable or its trust boundary drifted.' }
} finally {
    for ($index=$held.Count-1; $index -ge 0; $index--) { $held[$index].Dispose() }
}

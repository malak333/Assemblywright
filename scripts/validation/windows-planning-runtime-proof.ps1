[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9_.-]{1,128}$')][string]$ServiceName = 'AssemblywrightMaster',
    [switch]$Confirm
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'The Windows planning environment diagnostic requires -Confirm.' }
$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -eq $service -or $service.Status -ne 'Stopped') { throw 'The exact Assemblywright service must be installed and stopped.' }

if (-not ('AssemblywrightPlanningEnvironmentDiagnosticV5' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class AssemblywrightPlanningEnvironmentDiagnosticV5 {
  public const int ContractVersion=5;
  const uint TOKEN_QUERY=0x0008,TOKEN_DUPLICATE=0x0002,TOKEN_ASSIGN_PRIMARY=0x0001,DISABLE_MAX_PRIVILEGE=0x1;
  const uint EXTENDED_STARTUPINFO_PRESENT=0x00080000,CREATE_SUSPENDED=0x00000004,CREATE_NO_WINDOW=0x08000000,CREATE_UNICODE_ENVIRONMENT=0x00000400;
  const int PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES=0x00020009;
  [StructLayout(LayoutKind.Sequential)] struct SID_AND_ATTRIBUTES { public IntPtr Sid; public uint Attributes; }
  [StructLayout(LayoutKind.Sequential)] struct SECURITY_CAPABILITIES { public IntPtr AppContainerSid,Capabilities; public uint CapabilityCount,Reserved; }
  [StructLayout(LayoutKind.Sequential)] struct STARTUPINFO { public int cb; public IntPtr reserved,desktop,title; public int x,y,xSize,ySize,xCountChars,yCountChars,fillAttribute,flags; public short showWindow,reserved2; public IntPtr reserved3,stdInput,stdOutput,stdError; }
  [StructLayout(LayoutKind.Sequential)] struct STARTUPINFOEX { public STARTUPINFO StartupInfo; public IntPtr AttributeList; }
  [StructLayout(LayoutKind.Sequential)] struct PROCESS_INFORMATION { public IntPtr Process,Thread; public uint ProcessId,ThreadId; }
  [StructLayout(LayoutKind.Sequential)] struct JOBOBJECT_BASIC_LIMIT_INFORMATION { public long PerProcessUserTimeLimit,PerJobUserTimeLimit; public uint LimitFlags; public UIntPtr MinimumWorkingSetSize,MaximumWorkingSetSize; public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass,SchedulingClass; }
  [StructLayout(LayoutKind.Sequential)] struct IO_COUNTERS { public ulong ReadOperationCount,WriteOperationCount,OtherOperationCount,ReadTransferCount,WriteTransferCount,OtherTransferCount; }
  [StructLayout(LayoutKind.Sequential)] struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION { public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation; public IO_COUNTERS IoInfo; public UIntPtr ProcessMemoryLimit,JobMemoryLimit,PeakProcessMemoryUsed,PeakJobMemoryUsed; }
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern uint GetWindowsDirectory(StringBuilder p,uint z);
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern uint GetSystemDirectory(StringBuilder p,uint z);
  [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr p,uint a,out IntPtr t);
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool CreateRestrictedToken(IntPtr e,uint f,uint dc,IntPtr ds,uint dp,IntPtr ps,uint dr,IntPtr rs,out IntPtr n);
  [DllImport("userenv.dll",CharSet=CharSet.Unicode)] static extern int DeriveAppContainerSidFromAppContainerName(string n,out IntPtr s);
  [DllImport("userenv.dll",SetLastError=true)] static extern bool CreateEnvironmentBlock(out IntPtr e,IntPtr t,bool inherit);
  [DllImport("userenv.dll",SetLastError=true)] static extern bool DestroyEnvironmentBlock(IntPtr e);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertStringSidToSid(string s,out IntPtr p);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr l,int c,uint f,ref IntPtr z);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr l,uint f,IntPtr a,IntPtr v,IntPtr z,IntPtr p,IntPtr r);
  [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr l);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessAsUser(IntPtr t,string a,StringBuilder c,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref STARTUPINFOEX s,out PROCESS_INFORMATION p);
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr a,string n);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(IntPtr j,int c,IntPtr i,uint z);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr j,IntPtr p);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateProcess(IntPtr h,uint c);
  [DllImport("kernel32.dll",SetLastError=true)] static extern uint WaitForSingleObject(IntPtr h,uint m);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr p);
  [DllImport("advapi32.dll")] static extern IntPtr FreeSid(IntPtr s);
  public sealed class Result { public int Candidate; public int Win32; public int Count; public bool Success; }
  static void Required(bool ok,int stage) { if(!ok){int code=Marshal.GetLastWin32Error();throw new InvalidOperationException("diagnostic_stage_"+stage.ToString("D2")+"_win32_"+code,new Win32Exception(code));} }
  static string Directory(Func<StringBuilder,uint,uint> call,int stage) { StringBuilder value=new StringBuilder(32768); uint count=call(value,(uint)value.Capacity); if(count==0||count>=(uint)value.Capacity)throw new InvalidOperationException("diagnostic_stage_"+stage.ToString("D2")); return value.ToString(); }
  static IntPtr EnvironmentBlock(string[] entries) { string joined=String.Join("\0",entries)+"\0\0"; char[] units=joined.ToCharArray(); if(units.Length<2||units[units.Length-1]!='\0'||units[units.Length-2]!='\0')throw new InvalidOperationException("diagnostic_stage_08_environment_shape"); IntPtr block=Marshal.AllocHGlobal(units.Length*2); Marshal.Copy(units,0,block,units.Length); return block; }
  static string[] ReadEnvironmentBlock(IntPtr block) { List<string> entries=new List<string>(); int offset=0,total=0; while(true){if(total++>65536||entries.Count>512)throw new InvalidOperationException("diagnostic_stage_22_environment_bounds"); char first=(char)Marshal.ReadInt16(block,offset*2);if(first=='\0')break;StringBuilder entry=new StringBuilder();while(true){if(total++>65536)throw new InvalidOperationException("diagnostic_stage_22_environment_bounds");char value=(char)Marshal.ReadInt16(block,offset++*2);if(value=='\0')break;entry.Append(value);}entries.Add(entry.ToString());}return entries.ToArray(); }
  static int Group(string name) { if(name.Length==3&&name[0]=='='&&Char.IsLetter(name[1])&&name[2]==':')return 1; switch(name.ToUpperInvariant()){case "SYSTEMROOT":return 1;case "COMSPEC":case "OS":case "PATH":case "PATHEXT":case "SYSTEMDRIVE":case "WINDIR":return 2;case "APPDATA":case "HOMEDRIVE":case "HOMEPATH":case "LOCALAPPDATA":case "TEMP":case "TMP":case "USERDOMAIN":case "USERNAME":case "USERPROFILE":return 4;case "ALLUSERSPROFILE":case "COMMONPROGRAMFILES":case "COMMONPROGRAMFILES(X86)":case "COMMONPROGRAMW6432":case "DRIVERDATA":case "PROGRAMDATA":case "PROGRAMFILES":case "PROGRAMFILES(X86)":case "PROGRAMW6432":case "PUBLIC":return 8;case "NUMBER_OF_PROCESSORS":case "PROCESSOR_ARCHITECTURE":case "PROCESSOR_IDENTIFIER":case "PROCESSOR_LEVEL":case "PROCESSOR_REVISION":return 16;default:return 0;} }
  static string Name(string entry) { if(entry.Length<3)return String.Empty;int separator=entry[0]=='='?entry.IndexOf('=',1):entry.IndexOf('=');return separator>0?entry.Substring(0,separator):String.Empty; }
  static int ProfileBit(string name) { switch(name.ToUpperInvariant()){case "APPDATA":return 1;case "HOMEDRIVE":return 2;case "HOMEPATH":return 4;case "LOCALAPPDATA":return 8;case "TEMP":return 16;case "TMP":return 32;case "USERDOMAIN":return 64;case "USERNAME":return 128;case "USERPROFILE":return 256;default:return 0;} }
  static string[] Filter(string[] source,int mask,int profileSelection) { List<string> pseudo=new List<string>(),normal=new List<string>();foreach(string entry in source){string name=Name(entry);int group=Group(name);if(group==0||(group&mask)==0||(group==4&&(ProfileBit(name)&profileSelection)==0))continue;if(name[0]=='=')pseudo.Add(entry);else normal.Add(entry);}pseudo.Sort(StringComparer.OrdinalIgnoreCase);normal.Sort(StringComparer.OrdinalIgnoreCase);pseudo.AddRange(normal);return pseudo.ToArray(); }
  public static Result Try(int candidate,int tokenMode,int environmentMode,bool nullCurrentDirectory,int filterMask,int profileSelection) {
    if(candidate<1||candidate>6)throw new ArgumentOutOfRangeException("candidate");
    if(tokenMode<0||tokenMode>1||environmentMode<0||environmentMode>3||filterMask<0||filterMask>31||profileSelection<0||profileSelection>511)throw new ArgumentOutOfRangeException("mode");
    string windows=Directory(GetWindowsDirectory,1),system=Directory(GetSystemDirectory,2),drive=System.IO.Path.GetPathRoot(windows).TrimEnd('\\');
    string powershell=System.IO.Path.Combine(system,"WindowsPowerShell","v1.0","powershell.exe"),comspec=System.IO.Path.Combine(system,"cmd.exe");
    List<string> normalEntries=new List<string>(); normalEntries.Add("SystemRoot="+windows);
    if((candidate>=2&&candidate<=4)||candidate==6)normalEntries.Add("SystemDrive="+drive);
    if((candidate>=3&&candidate<=4)||candidate==6){normalEntries.Add("ComSpec="+comspec);normalEntries.Add("windir="+windows);}
    if(candidate==4||candidate==6){string temporary=System.IO.Path.Combine(windows,"Temp");normalEntries.Add("TEMP="+temporary);normalEntries.Add("TMP="+temporary);}
    normalEntries.Sort(StringComparer.OrdinalIgnoreCase); List<string> entries=new List<string>(); if(candidate>=5)entries.Add("="+drive+"="+windows); entries.AddRange(normalEntries);
    IntPtr environment=IntPtr.Zero,userenvSource=IntPtr.Zero,baseToken=IntPtr.Zero,restricted=IntPtr.Zero,profileSid=IntPtr.Zero,internetSid=IntPtr.Zero,capabilityMemory=IntPtr.Zero,securityMemory=IntPtr.Zero,list=IntPtr.Zero,job=IntPtr.Zero,jobMemory=IntPtr.Zero; bool environmentFromUserenv=false;int environmentCount=0; PROCESS_INFORMATION process=new PROCESS_INFORMATION();
    try {
      if(environmentMode==1){environment=EnvironmentBlock(entries.ToArray());environmentCount=entries.Count;} if(tokenMode==1){Required(OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY|TOKEN_DUPLICATE|TOKEN_ASSIGN_PRIMARY,out baseToken),10); Required(CreateRestrictedToken(baseToken,DISABLE_MAX_PRIVILEGE,0,IntPtr.Zero,0,IntPtr.Zero,0,IntPtr.Zero,out restricted),11);} if(environmentMode==2||environmentMode==3){if(!CreateEnvironmentBlock(out userenvSource,restricted,false))return new Result { Candidate=candidate,Win32=Marshal.GetLastWin32Error(),Count=0,Success=false };if(environmentMode==2){environment=userenvSource;environmentFromUserenv=true;environmentCount=ReadEnvironmentBlock(environment).Length;userenvSource=IntPtr.Zero;}else{string[] filtered=Filter(ReadEnvironmentBlock(userenvSource),filterMask,profileSelection);environment=EnvironmentBlock(filtered);environmentCount=filtered.Length;}}
      int hr=DeriveAppContainerSidFromAppContainerName("Assemblywright.Planning.Provider.v1",out profileSid); if(hr<0)throw new InvalidOperationException("diagnostic_stage_12_hresult_"+unchecked((uint)hr).ToString("X8")); Required(ConvertStringSidToSid("S-1-15-3-1",out internetSid),13);
      SID_AND_ATTRIBUTES capability=new SID_AND_ATTRIBUTES { Sid=internetSid,Attributes=4 }; capabilityMemory=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(SID_AND_ATTRIBUTES))); Marshal.StructureToPtr(capability,capabilityMemory,false); SECURITY_CAPABILITIES security=new SECURITY_CAPABILITIES { AppContainerSid=profileSid,Capabilities=capabilityMemory,CapabilityCount=1,Reserved=0 }; securityMemory=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(SECURITY_CAPABILITIES))); Marshal.StructureToPtr(security,securityMemory,false);
      IntPtr bytes=IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref bytes); if(bytes==IntPtr.Zero)throw new InvalidOperationException("diagnostic_stage_14"); list=Marshal.AllocHGlobal(bytes); Required(InitializeProcThreadAttributeList(list,1,0,ref bytes),15); Required(UpdateProcThreadAttribute(list,0,(IntPtr)PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,securityMemory,(IntPtr)Marshal.SizeOf(typeof(SECURITY_CAPABILITIES)),IntPtr.Zero,IntPtr.Zero),16);
      STARTUPINFOEX startup=new STARTUPINFOEX(); startup.StartupInfo.cb=Marshal.SizeOf(typeof(STARTUPINFOEX)); startup.AttributeList=list; StringBuilder command=new StringBuilder("\""+powershell+"\" -NoLogo -NoProfile -NonInteractive -Command exit 0");
      IntPtr launchToken=tokenMode==1?restricted:IntPtr.Zero; string currentDirectory=nullCurrentDirectory?null:windows; bool created=CreateProcessAsUser(launchToken,powershell,command,IntPtr.Zero,IntPtr.Zero,false,EXTENDED_STARTUPINFO_PRESENT|CREATE_SUSPENDED|CREATE_NO_WINDOW|CREATE_UNICODE_ENVIRONMENT,environment,currentDirectory,ref startup,out process); if(!created)return new Result { Candidate=candidate,Win32=Marshal.GetLastWin32Error(),Count=environmentCount,Success=false };
      job=CreateJobObject(IntPtr.Zero,null); Required(job!=IntPtr.Zero,17); JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits=new JOBOBJECT_EXTENDED_LIMIT_INFORMATION(); limits.BasicLimitInformation.LimitFlags=0x00002000; jobMemory=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION))); Marshal.StructureToPtr(limits,jobMemory,false); Required(SetInformationJobObject(job,9,jobMemory,(uint)Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION))),18); Required(AssignProcessToJobObject(job,process.Process),19); Required(TerminateProcess(process.Process,0xA55E3001),20); if(WaitForSingleObject(process.Process,5000)!=0)throw new InvalidOperationException("diagnostic_stage_21_wait"); return new Result { Candidate=candidate,Win32=0,Count=environmentCount,Success=true };
    } finally { if(job!=IntPtr.Zero)CloseHandle(job);if(process.Process!=IntPtr.Zero){TerminateProcess(process.Process,0xA55E3002);CloseHandle(process.Process);}if(process.Thread!=IntPtr.Zero)CloseHandle(process.Thread);if(list!=IntPtr.Zero){DeleteProcThreadAttributeList(list);Marshal.FreeHGlobal(list);}if(jobMemory!=IntPtr.Zero)Marshal.FreeHGlobal(jobMemory);if(securityMemory!=IntPtr.Zero)Marshal.FreeHGlobal(securityMemory);if(capabilityMemory!=IntPtr.Zero)Marshal.FreeHGlobal(capabilityMemory);if(internetSid!=IntPtr.Zero)LocalFree(internetSid);if(profileSid!=IntPtr.Zero)FreeSid(profileSid);if(restricted!=IntPtr.Zero)CloseHandle(restricted);if(baseToken!=IntPtr.Zero)CloseHandle(baseToken);if(userenvSource!=IntPtr.Zero)DestroyEnvironmentBlock(userenvSource);if(environment!=IntPtr.Zero){if(environmentFromUserenv)DestroyEnvironmentBlock(environment);else Marshal.FreeHGlobal(environment);}}
  }
}
'@
}
if ([AssemblywrightPlanningEnvironmentDiagnosticV5]::ContractVersion -ne 5) { throw 'The loaded planning environment diagnostic contract has the wrong version.' }

$results = @()
$minimal = 0
for ($candidate=1; $candidate -le 6; $candidate++) {
    $result = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try($candidate,1,1,$false,0,511)
    $results += [ordered]@{ candidate=$result.Candidate; success=$result.Success; win32=$result.Win32; count=$result.Count }
    if ($result.Success) { $minimal = $candidate; break }
}

# Diagnostic-only token matrix. Microsoft SandboxSecurityTools LaunchAppContainer calls
# CreateProcessAsUser(NULL, ...) with AppContainer security capabilities. NULL-environment cases
# can inherit inside CreateProcessAsUser, but every child remains suspended and is terminated
# before resume; these cases are evidence only and must never become the production contract.
# https://github.com/microsoft/SandboxSecurityTools/tree/main/LaunchAppContainer
$tokenResults = @()
function Invoke-TokenCase([int]$Case,[int]$TokenMode,[int]$EnvironmentMode,[bool]$NullCurrentDirectory) {
    $result = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try(4,$TokenMode,$EnvironmentMode,$NullCurrentDirectory,0,511)
    $script:tokenResults += [ordered]@{ case=$Case; success=$result.Success; win32=$result.Win32; count=$result.Count }
    return $result.Success
}
$null = Invoke-TokenCase 1 1 1 $false
$nullTokenExplicit = Invoke-TokenCase 2 0 1 $false
if (-not $nullTokenExplicit) {
    $null = Invoke-TokenCase 3 0 0 $false
    $null = Invoke-TokenCase 4 1 0 $false
}
if (-not (@($tokenResults | Where-Object { $_.success })).Count) {
    $null = Invoke-TokenCase 5 0 1 $true
    $null = Invoke-TokenCase 6 1 1 $true
}
$userenvResults = @()
$userenv = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try(4,1,2,$false,0,511)
$userenvResults += [ordered]@{ case=7; success=$userenv.Success; win32=$userenv.Win32; count=$userenv.Count }
if ($userenv.Success) {
    $filtered = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try(4,1,3,$false,31,511)
    $userenvResults += [ordered]@{ case=8; success=$filtered.Success; win32=$filtered.Win32; count=$filtered.Count }
    if ($filtered.Success) {
        # Fixed group probes: 1=SystemRoot+pseudo, 2=process OS, 4=user profile,
        # 8=program/shared roots, 16=processor metadata. Case IDs make results interpretable
        # without disclosing any environment name or value.
        foreach ($probe in @(
            @{ Case=9; Mask=3 }, @{ Case=10; Mask=5 }, @{ Case=11; Mask=9 },
            @{ Case=12; Mask=17 }, @{ Case=13; Mask=1 }, @{ Case=14; Mask=29 },
            @{ Case=15; Mask=27 }, @{ Case=16; Mask=23 }, @{ Case=17; Mask=15 }
        )) {
            $probeResult = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try(4,1,3,$false,$probe.Mask,511)
            $userenvResults += [ordered]@{ case=$probe.Case; success=$probeResult.Success; win32=$probeResult.Win32; count=$probeResult.Count }
        }
        # Singleton case IDs 18-26 are fixed in this exact order:
        # APPDATA,HOMEDRIVE,HOMEPATH,LOCALAPPDATA,TEMP,TMP,USERDOMAIN,USERNAME,USERPROFILE.
        $singletonSuccess = $false
        for ($profileIndex=0; $profileIndex -lt 9; $profileIndex++) {
            $selection = 1 -shl $profileIndex
            $singleton = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try(4,1,3,$false,5,$selection)
            $userenvResults += [ordered]@{ case=(18+$profileIndex); success=$singleton.Success; win32=$singleton.Win32; count=$singleton.Count }
            if ($singleton.Success) { $singletonSuccess = $true }
        }
        if (-not $singletonSuccess) {
            # Pair case IDs 27-62 are the lexicographic (0,1),(0,2)...(7,8) combinations
            # of the singleton order above. The fixed IDs reveal only which safe allowlist slots
            # are required; values and names are never emitted by the diagnostic result.
            $pairCase = 27
            for ($left=0; $left -lt 8; $left++) {
                for ($right=$left+1; $right -lt 9; $right++) {
                    $selection = (1 -shl $left) -bor (1 -shl $right)
                    $pair = [AssemblywrightPlanningEnvironmentDiagnosticV5]::Try(4,1,3,$false,5,$selection)
                    $userenvResults += [ordered]@{ case=$pairCase; success=$pair.Success; win32=$pair.Win32; count=$pair.Count }
                    $pairCase++
                }
            }
            if ($pairCase -ne 63) { throw 'The fixed profile pair diagnostic matrix changed.' }
        }
    }
}
[ordered]@{ status='planning_environment_diagnostic_complete'; minimal_success=$minimal; environment_results=$results; token_results=$tokenResults; userenv_results=$userenvResults } | ConvertTo-Json -Compress -Depth 5

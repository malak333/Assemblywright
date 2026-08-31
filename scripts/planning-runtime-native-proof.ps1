[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$DataDir,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-fA-F]{64}$')][string]$MasterExeSha256,
    [string]$ServiceName = 'AssemblywrightMaster',
    [switch]$Confirm
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $Confirm) { throw 'Native planning containment proof requires -Confirm.' }
$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -ne $service -and $service.Status -ne 'Stopped') { throw 'Run the native proof only while the service is stopped.' }
& (Join-Path $PSScriptRoot 'check-planning-runtime.ps1') -DataDir $DataDir -MasterExeSha256 $MasterExeSha256 -ServiceName $ServiceName -Confirm
if ($LASTEXITCODE -ne 0) { throw 'Static planning containment proof failed.' }
# Execute the exact check twice in this PowerShell 5.1 process. This proves the versioned Add-Type
# guard and catches global-type collisions that would otherwise make an idempotent owner check fail.
& (Join-Path $PSScriptRoot 'check-planning-runtime.ps1') -DataDir $DataDir -MasterExeSha256 $MasterExeSha256 -ServiceName $ServiceName -Confirm
if ($LASTEXITCODE -ne 0) { throw 'Same-process planning containment recheck failed.' }

if (-not ('AssemblywrightPlanningAppContainerLifecycleProofV2' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class AssemblywrightPlanningAppContainerLifecycleProofV2 {
  public const int ContractVersion=2;
  const uint EXTENDED_STARTUPINFO_PRESENT=0x00080000,CREATE_SUSPENDED=0x00000004,CREATE_NO_WINDOW=0x08000000,CREATE_UNICODE_ENVIRONMENT=0x00000400;
  const int PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES=0x00020009,TokenIsAppContainer=29,TokenAppContainerSid=31;
  const uint TOKEN_QUERY=0x0008,TOKEN_DUPLICATE=0x0002,TOKEN_ASSIGN_PRIMARY=0x0001,DISABLE_MAX_PRIVILEGE=0x1,WAIT_OBJECT_0=0,WAIT_TIMEOUT=258;
  [StructLayout(LayoutKind.Sequential)] struct SECURITY_CAPABILITIES { public IntPtr AppContainerSid,Capabilities; public uint CapabilityCount,Reserved; }
  [StructLayout(LayoutKind.Sequential)] struct SID_AND_ATTRIBUTES { public IntPtr Sid; public uint Attributes; }
  [StructLayout(LayoutKind.Sequential)] struct STARTUPINFO { public int cb; public IntPtr reserved,desktop,title; public int x,y,xSize,ySize,xCountChars,yCountChars,fillAttribute,flags; public short showWindow,reserved2; public IntPtr reserved3,stdInput,stdOutput,stdError; }
  [StructLayout(LayoutKind.Sequential)] struct STARTUPINFOEX { public STARTUPINFO StartupInfo; public IntPtr AttributeList; }
  [StructLayout(LayoutKind.Sequential)] struct PROCESS_INFORMATION { public IntPtr Process,Thread; public uint ProcessId,ThreadId; }
  [StructLayout(LayoutKind.Sequential)] struct TOKEN_APPCONTAINER_INFORMATION { public IntPtr TokenAppContainer; }
  [StructLayout(LayoutKind.Sequential)] struct JOBOBJECT_BASIC_LIMIT_INFORMATION { public long PerProcessUserTimeLimit,PerJobUserTimeLimit; public uint LimitFlags; public UIntPtr MinimumWorkingSetSize,MaximumWorkingSetSize; public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass,SchedulingClass; }
  [StructLayout(LayoutKind.Sequential)] struct IO_COUNTERS { public ulong ReadOperationCount,WriteOperationCount,OtherOperationCount,ReadTransferCount,WriteTransferCount,OtherTransferCount; }
  [StructLayout(LayoutKind.Sequential)] struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION { public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation; public IO_COUNTERS IoInfo; public UIntPtr ProcessMemoryLimit,JobMemoryLimit,PeakProcessMemoryUsed,PeakJobMemoryUsed; }
  [DllImport("userenv.dll",CharSet=CharSet.Unicode)] static extern int DeriveAppContainerSidFromAppContainerName(string n,out IntPtr s);
  [DllImport("userenv.dll",SetLastError=true)] static extern bool CreateEnvironmentBlock(out IntPtr e,IntPtr token,bool inherit);
  [DllImport("userenv.dll",SetLastError=true)] static extern bool DestroyEnvironmentBlock(IntPtr e);
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool EqualSid(IntPtr a,IntPtr b);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertStringSidToSid(string s,out IntPtr p);
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool OpenProcessToken(IntPtr p,uint a,out IntPtr t);
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool CreateRestrictedToken(IntPtr e,uint f,uint dc,IntPtr ds,uint dp,IntPtr ps,uint dr,IntPtr rs,out IntPtr n);
  [DllImport("advapi32.dll",SetLastError=true)] static extern bool GetTokenInformation(IntPtr t,int c,IntPtr b,uint z,out uint n);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr l,int c,uint f,ref IntPtr z);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr l,uint f,IntPtr a,IntPtr v,IntPtr z,IntPtr p,IntPtr r);
  [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr l);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessAsUser(IntPtr t,string a,StringBuilder c,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref STARTUPINFOEX s,out PROCESS_INFORMATION p);
  [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
  [DllImport("kernel32.dll",SetLastError=true)] static extern uint WaitForSingleObject(IntPtr h,uint m);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateProcess(IntPtr h,uint c);
  [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern IntPtr CreateJobObject(IntPtr a,string n);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(IntPtr j,int c,IntPtr i,uint z);
  [DllImport("kernel32.dll",SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr j,IntPtr p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("advapi32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool ConvertSidToStringSid(IntPtr s,out IntPtr p);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr p);
  [DllImport("advapi32.dll")] static extern IntPtr FreeSid(IntPtr s);
  static void Stage(bool ok,int stage) { if(!ok){int code=Marshal.GetLastWin32Error();throw new InvalidOperationException("native_stage_"+stage.ToString("D2")+"_win32_"+code,new Win32Exception(code));} }
  static void HResultStage(int hr,int stage) { if(hr<0)throw new InvalidOperationException("native_stage_"+stage.ToString("D2")+"_hresult_"+unchecked((uint)hr).ToString("X8"),Marshal.GetExceptionForHR(hr)); }
  static string ExtractLocalAppData(IntPtr source) {
    const int MaxUnits=32768,MaxEntries=512; StringBuilder entry=new StringBuilder(); string local=null; bool entryTerminated=false; int entries=0;
    for(int index=0;index<MaxUnits;index++) {
      char unit=(char)Marshal.ReadInt16(source,index*2);
      if(unit!='\0'){entry.Append(unit);entryTerminated=false;continue;}
      if(entry.Length==0){if(!entryTerminated)throw new InvalidOperationException("native_stage_32_environment_malformed");if(local==null)throw new InvalidOperationException("native_stage_33_localappdata_missing");return local;}
      entries++;if(entries>MaxEntries)throw new InvalidOperationException("native_stage_34_environment_oversize");string value=entry.ToString();entry.Clear();entryTerminated=true;
      int separator=value[0]=='='?value.IndexOf('=',1):value.IndexOf('=');if(separator<=0||separator==value.Length-1)throw new InvalidOperationException("native_stage_32_environment_malformed");
      string name=value.Substring(0,separator);if(String.Equals(name,"LOCALAPPDATA",StringComparison.OrdinalIgnoreCase)){if(local!=null)throw new InvalidOperationException("native_stage_35_localappdata_duplicate");local=value.Substring(separator+1);if(local.Length==0)throw new InvalidOperationException("native_stage_32_environment_malformed");}
    }
    throw new InvalidOperationException("native_stage_34_environment_oversize");
  }
  static IntPtr ExactEnvironment(IntPtr restrictedToken,string systemRoot) {
    if(String.IsNullOrEmpty(systemRoot)||systemRoot.IndexOf('\0')>=0)throw new InvalidOperationException("native_stage_08_environment_shape");IntPtr source=IntPtr.Zero;Stage(CreateEnvironmentBlock(out source,restrictedToken,false)&&source!=IntPtr.Zero,30);string local;
    try{local=ExtractLocalAppData(source);}finally{Stage(DestroyEnvironmentBlock(source),31);}
    char[] block=("LOCALAPPDATA="+local+'\0'+"SystemRoot="+systemRoot+'\0'+'\0').ToCharArray();if(block.Length<2||block[block.Length-1]!='\0'||block[block.Length-2]!='\0')throw new InvalidOperationException("native_stage_08_environment_shape");IntPtr exact=Marshal.AllocHGlobal(block.Length*2);Marshal.Copy(block,0,exact,block.Length);return exact;
  }
  public static string Sid(string profile) { IntPtr sid; int hr=DeriveAppContainerSidFromAppContainerName(profile,out sid); HResultStage(hr,1); try{IntPtr text;Stage(ConvertSidToStringSid(sid,out text),2);try{return Marshal.PtrToStringUni(text);}finally{LocalFree(text);}}finally{FreeSid(sid);} }
  public static uint Run(string profile,string executable,string systemRoot,string workingDirectory) {
    IntPtr sid=IntPtr.Zero,capabilitySid=IntPtr.Zero,capabilityMemory=IntPtr.Zero,list=IntPtr.Zero,securityMemory=IntPtr.Zero,environment=IntPtr.Zero,baseToken=IntPtr.Zero,restrictedToken=IntPtr.Zero,token=IntPtr.Zero,tokenBuffer=IntPtr.Zero,job=IntPtr.Zero,jobMemory=IntPtr.Zero; PROCESS_INFORMATION pi=new PROCESS_INFORMATION();
    try {
      int hr=DeriveAppContainerSidFromAppContainerName(profile,out sid); HResultStage(hr,10);
      IntPtr bytes=IntPtr.Zero; InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref bytes); if(bytes==IntPtr.Zero)throw new InvalidOperationException("native_stage_11"); list=Marshal.AllocHGlobal(bytes); Stage(InitializeProcThreadAttributeList(list,1,0,ref bytes),12);
      Stage(ConvertStringSidToSid("S-1-15-3-1",out capabilitySid),9); SID_AND_ATTRIBUTES capability=new SID_AND_ATTRIBUTES { Sid=capabilitySid,Attributes=0x00000004 }; capabilityMemory=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(SID_AND_ATTRIBUTES))); Marshal.StructureToPtr(capability,capabilityMemory,false);
      SECURITY_CAPABILITIES security=new SECURITY_CAPABILITIES { AppContainerSid=sid,Capabilities=capabilityMemory,CapabilityCount=1,Reserved=0 };
      securityMemory=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(SECURITY_CAPABILITIES))); Marshal.StructureToPtr(security,securityMemory,false);
      Stage(UpdateProcThreadAttribute(list,0,(IntPtr)PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,securityMemory,(IntPtr)Marshal.SizeOf(typeof(SECURITY_CAPABILITIES)),IntPtr.Zero,IntPtr.Zero),13);
      STARTUPINFOEX startup=new STARTUPINFOEX(); startup.StartupInfo.cb=Marshal.SizeOf(typeof(STARTUPINFOEX)); startup.AttributeList=list;
      StringBuilder command=new StringBuilder("\""+executable+"\"");
      Stage(OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY|TOKEN_DUPLICATE|TOKEN_ASSIGN_PRIMARY,out baseToken),14); Stage(CreateRestrictedToken(baseToken,DISABLE_MAX_PRIVILEGE,0,IntPtr.Zero,0,IntPtr.Zero,0,IntPtr.Zero,out restrictedToken),15);
      environment=ExactEnvironment(restrictedToken,systemRoot);
      Stage(CreateProcessAsUser(restrictedToken,executable,command,IntPtr.Zero,IntPtr.Zero,false,EXTENDED_STARTUPINFO_PRESENT|CREATE_SUSPENDED|CREATE_NO_WINDOW|CREATE_UNICODE_ENVIRONMENT,environment,workingDirectory,ref startup,out pi),16);
      job=CreateJobObject(IntPtr.Zero,null); Stage(job!=IntPtr.Zero,17); JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits=new JOBOBJECT_EXTENDED_LIMIT_INFORMATION(); limits.BasicLimitInformation.LimitFlags=0x00002000; jobMemory=Marshal.AllocHGlobal(Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION))); Marshal.StructureToPtr(limits,jobMemory,false); Stage(SetInformationJobObject(job,9,jobMemory,(uint)Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION))),18); Stage(AssignProcessToJobObject(job,pi.Process),19);
      Stage(OpenProcessToken(pi.Process,TOKEN_QUERY,out token),20); uint needed; int value=0; IntPtr valuePointer=Marshal.AllocHGlobal(sizeof(int)); try{Stage(GetTokenInformation(token,TokenIsAppContainer,valuePointer,sizeof(int),out needed),21);value=Marshal.ReadInt32(valuePointer);}finally{Marshal.FreeHGlobal(valuePointer);} if(value!=1)throw new InvalidOperationException("native_stage_22_child token category mismatch");
      GetTokenInformation(token,TokenAppContainerSid,IntPtr.Zero,0,out needed); if(needed<(uint)IntPtr.Size)throw new InvalidOperationException("native_stage_23_token_sid_size"); tokenBuffer=Marshal.AllocHGlobal((int)needed); Stage(GetTokenInformation(token,TokenAppContainerSid,tokenBuffer,needed,out needed),24); TOKEN_APPCONTAINER_INFORMATION observed=(TOKEN_APPCONTAINER_INFORMATION)Marshal.PtrToStructure(tokenBuffer,typeof(TOKEN_APPCONTAINER_INFORMATION)); if(observed.TokenAppContainer==IntPtr.Zero||!EqualSid(sid,observed.TokenAppContainer))throw new InvalidOperationException("native_stage_25_child profile mismatch");
      Stage(TerminateProcess(pi.Process,0xA55E2001),26); uint wait=WaitForSingleObject(pi.Process,5000); if(wait==WAIT_TIMEOUT)throw new TimeoutException("native_stage_27_timeout"); if(wait!=WAIT_OBJECT_0)Stage(false,28); return 0;
    } finally { if(job!=IntPtr.Zero)CloseHandle(job);if(pi.Process!=IntPtr.Zero){TerminateProcess(pi.Process,0xA55E2002);CloseHandle(pi.Process);}if(pi.Thread!=IntPtr.Zero)CloseHandle(pi.Thread);if(token!=IntPtr.Zero)CloseHandle(token);if(restrictedToken!=IntPtr.Zero)CloseHandle(restrictedToken);if(baseToken!=IntPtr.Zero)CloseHandle(baseToken);if(tokenBuffer!=IntPtr.Zero)Marshal.FreeHGlobal(tokenBuffer);if(jobMemory!=IntPtr.Zero)Marshal.FreeHGlobal(jobMemory);if(list!=IntPtr.Zero){DeleteProcThreadAttributeList(list);Marshal.FreeHGlobal(list);}if(securityMemory!=IntPtr.Zero)Marshal.FreeHGlobal(securityMemory);if(capabilityMemory!=IntPtr.Zero)Marshal.FreeHGlobal(capabilityMemory);if(capabilitySid!=IntPtr.Zero)LocalFree(capabilitySid);if(environment!=IntPtr.Zero)Marshal.FreeHGlobal(environment);if(sid!=IntPtr.Zero)FreeSid(sid); }
  }
}
'@
}
if ([AssemblywrightPlanningAppContainerLifecycleProofV2]::ContractVersion -ne 2) { throw 'The loaded AppContainer lifecycle proof contract has the wrong version.' }

$data = [IO.Path]::GetFullPath($DataDir).TrimEnd('\')
$programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
if ([string]::IsNullOrWhiteSpace($programData)) { throw 'The canonical common application data root is unavailable.' }
$runtimeRoot = Join-Path (Join-Path (Join-Path $programData 'Assemblywright') 'planning-runtime') $ServiceName
$providerRoot = Join-Path $runtimeRoot 'provider'
$provider = Join-Path $providerRoot 'brainstorming-provider.exe'
if (-not (Test-Path -LiteralPath $provider -PathType Leaf)) { throw 'The staged planning provider is unavailable.' }
$exit = [AssemblywrightPlanningAppContainerLifecycleProofV2]::Run('Assemblywright.Planning.Provider.v1',$provider,$env:SystemRoot,$providerRoot)
if ($exit -ne 0) { throw "native_child_exit_$exit" }
& (Join-Path $PSScriptRoot 'check-planning-runtime.ps1') -DataDir $DataDir -MasterExeSha256 $MasterExeSha256 -ServiceName $ServiceName -Confirm
if ($LASTEXITCODE -ne 0) { throw 'Planning runtime drifted after native containment proof.' }
# This proof establishes suspended creation under the exact provider AppContainer SID, explicit
# environment construction, Job binding, token observation, termination, and bounded wait without
# executing provider instructions. Writable create-close-reopen remains a real-provider live E2E.
# Outbound Codex and authenticated GitHub also remain owner-recorded live proofs.
Write-Output '{"status":"planning_runtime_native_process_containment_proof_passed","live_evidence_required":true}'

#[test]
fn private_desktop_launch_contract_is_create_only_closed_and_pre_spawn_restored() {
    let source = include_str!("../src/planning_runtime/windows_containment.rs");
    let cargo = include_str!("../Cargo.toml");

    for required in [
        "PRIVATE_WINDOW_STATION_CREATE_ONLY: u32 = 1",
        "WINDOW_STATION_PROFILE_ACCESS: u32 = 0x000f_006e",
        "DESKTOP_PROFILE_ACCESS: u32 = 0x000f_00cf",
        "bInheritHandle: 0",
        "CreateWindowStationW(",
        "CreateDesktopW(",
        "association.restore()?;",
        "startup.StartupInfo.lpDesktop = private_desktop.startup_name.as_ptr().cast_mut();",
        "std::process::abort();",
        "D:P(A;;0x{owner_access:08x};;;S-1-5-18)",
        "private_window_station_native_lifecycle_restores_original_association",
        "requires elevated native Windows staging",
    ] {
        assert!(source.contains(required), "missing contract: {required}");
    }
    assert!(cargo.contains("\"Win32_Graphics_Gdi\""));
    assert!(cargo.contains("\"Win32_System_StationsAndDesktops\""));
    let interactive_station = ["Win", "sta0"].concat();
    assert!(!source.contains(&interactive_station));
    assert!(!source.contains(";;;WD)"));
    assert!(!source.contains(";;;AC)"));

    let station = source.find("CreateWindowStationW(").unwrap();
    let desktop = source.find("CreateDesktopW(").unwrap();
    let restore = source.find("association.restore()?;").unwrap();
    let process = source.find("CreateProcessAsUserW(").unwrap();
    assert!(station < desktop && desktop < restore && restore < process);

    let inherited = source
        .find("let mut inherited = [stdin_read.raw(), stdout_write.raw(), stderr_write.raw()];")
        .unwrap();
    let handle_list = source
        .find("PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize")
        .unwrap();
    assert!(inherited < handle_list);
    assert!(source.contains("startup.StartupInfo.hStdError = stderr_write.raw();"));
    assert!(source.contains("let stderr_thread = discard_reader(stderr_file);"));
}

#[test]
fn private_desktop_failure_remains_pre_effect_and_content_free() {
    let source = include_str!("../src/planning_runtime/windows_containment.rs");
    let private_desktop = source.find("PrivateDesktop::create(profile)?").unwrap();
    let second_poll = source[private_desktop..]
        .find("if !control.poll()")
        .map(|offset| private_desktop + offset)
        .unwrap();
    let revalidate = source[second_poll..]
        .find("profile.revalidate()")
        .map(|offset| second_poll + offset)
        .unwrap();
    let process = source.find("CreateProcessAsUserW(").unwrap();
    assert!(private_desktop < second_poll && second_poll < revalidate && revalidate < process);
    assert!(!source.contains("private desktop failed:"));
    assert!(!source.contains("station_name, diagnostic"));
}

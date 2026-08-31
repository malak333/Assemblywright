#[cfg(windows)]
fn main() {
    use std::fs;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let descendant = std::env::args().nth(1).as_deref() == Some("--descendant");
    if descendant {
        loop {
            thread::sleep(Duration::from_secs(60));
        }
    }

    let executable = std::env::current_exe().expect("resolve helper image");
    let mut child = Command::new(executable)
        .arg("--descendant")
        .spawn()
        .expect("spawn contained descendant");
    fs::write(
        "job-started.txt",
        format!("root={} descendant={}", std::process::id(), child.id()),
    )
    .expect("write cwd marker");
    loop {
        if child
            .try_wait()
            .expect("observe contained descendant")
            .is_some()
        {
            panic!("contained descendant exited before Job termination");
        }
        thread::sleep(Duration::from_secs(60));
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows containment helper is unavailable on this host");
    std::process::exit(78);
}

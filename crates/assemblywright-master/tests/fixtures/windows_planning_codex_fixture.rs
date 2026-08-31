use std::io::{Read, Write};

fn main() {
    let mut prompt = Vec::new();
    std::io::stdin()
        .read_to_end(&mut prompt)
        .expect("read fixture prompt");
    assert!(!prompt.is_empty(), "fixture requires a real provider prompt");

    // This exceeds the usual anonymous-pipe capacity. The service proof can complete only when
    // the master supplies a valid inherited stderr handle and drains it concurrently without
    // retaining or returning the content.
    let diagnostic = [b'x'; 8192];
    let mut stderr = std::io::stderr().lock();
    for _ in 0..4 {
        stderr
            .write_all(&diagnostic)
            .expect("write bounded fixture stderr");
    }
    stderr.flush().expect("flush bounded fixture stderr");

    println!(
        "{{\"title\":\"Windows planning containment E2E\",\"outcome\":\"Return one schema-bound public plan.\",\"acceptance_criteria\":[{{\"id\":\"native-service-proof\",\"requirement\":\"The Windows service launches the provider inside the AppContainer.\"}}],\"obligations\":[\"Do not create a repository or enqueue work.\"]}}"
    );
}

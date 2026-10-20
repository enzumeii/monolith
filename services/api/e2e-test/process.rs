//! Owned test-child teardown, including children that ignore termination.

#[cfg(unix)]
#[test]
fn graceful_child_flushes_before_teardown() {
    let path = std::env::temp_dir().join(format!("api-flush-{}", uuid::Uuid::new_v4()));
    let mut child = std::process::Command::new("bash")
        .args([
            "-c",
            "trap 'echo flushed > \"$1\"; exit 0' TERM; echo ready; while :; do :; done",
            "bash",
        ])
        .arg(&path)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(child.stdout.take().unwrap()),
        &mut ready,
    )
    .unwrap();
    super::common::stop_process(&mut child, std::time::Duration::from_secs(2));
    assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), "flushed");
    assert!(child.try_wait().unwrap().unwrap().success());
    std::fs::remove_file(path).unwrap();
}

#[cfg(unix)]
#[test]
fn unresponsive_child_is_killed_and_reaped() {
    let mut child = std::process::Command::new("bash")
        .args(["-c", "trap '' TERM; echo ready; while :; do :; done"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut ready = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(child.stdout.take().unwrap()),
        &mut ready,
    )
    .unwrap();
    let start = std::time::Instant::now();
    super::common::stop_process(&mut child, std::time::Duration::from_millis(100));
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    assert!(!child.try_wait().unwrap().unwrap().success());
}

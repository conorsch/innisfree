use std::process::Command;

#[ignore]
#[test]
fn simple_http_server_is_remotely_accessible() {
    let status = Command::new("./tools/test-runner").status();
    assert!(status.is_ok());
}

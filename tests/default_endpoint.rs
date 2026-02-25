use predicates::str::contains;
use tempfile::tempdir;

#[test]
fn default_endpoint_uses_datadir_and_ipc_filename() {
    let dir = tempdir().expect("tempdir");
    let datadir = dir.path().to_string_lossy().to_string();

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("reth-console");
    cmd.arg("--datadir")
        .arg(&datadir)
        .arg("--exec")
        .arg("eth.blockNumber")
        .assert()
        .failure()
        .stderr(contains("reth.ipc"));
}

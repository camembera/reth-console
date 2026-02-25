use jsonrpsee::RpcModule;
use jsonrpsee::server::ServerBuilder;
use predicates::str::contains;

#[tokio::test(flavor = "multi_thread")]
async fn exec_prints_array_count() {
    let server = ServerBuilder::default()
        .build("127.0.0.1:0")
        .await
        .expect("server starts");
    let addr = server.local_addr().expect("local addr");

    let mut module = RpcModule::new(());
    module
        .register_method("eth_getLogs", |_params, _ctx, _ext| {
            Ok::<serde_json::Value, jsonrpsee::types::ErrorObjectOwned>(serde_json::json!([
                {"id": 1},
                {"id": 2},
                {"id": 3}
            ]))
        })
        .expect("register method");

    let handle = server.start(module);
    let endpoint = format!("http://{addr}");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("reth-console");
    cmd.arg("--exec")
        .arg("eth_getLogs []")
        .arg(endpoint)
        .assert()
        .success()
        .stdout(contains("3 items"));

    handle.stop().expect("stop server");
}

#[tokio::test(flavor = "multi_thread")]
async fn exec_prints_nested_list_counts() {
    let server = ServerBuilder::default()
        .build("127.0.0.1:0")
        .await
        .expect("server starts");
    let addr = server.local_addr().expect("local addr");

    let mut module = RpcModule::new(());
    module
        .register_method("eth_getBlockByNumber", |_params, _ctx, _ext| {
            Ok::<serde_json::Value, jsonrpsee::types::ErrorObjectOwned>(serde_json::json!({
                "number": "0x1",
                "transactions": [{"id": 1}, {"id": 2}, {"id": 3}]
            }))
        })
        .expect("register method");

    let handle = server.start(module);
    let endpoint = format!("http://{addr}");

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("reth-console");
    cmd.arg("--exec")
        .arg(r#"eth_getBlockByNumber ["latest", true]"#)
        .arg(endpoint)
        .assert()
        .success()
        .stdout(contains("$.transactions: 3 items"));

    handle.stop().expect("stop server");
}

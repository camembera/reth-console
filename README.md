# reth-console

`reth-console` is a standalone attach console for `reth` and `bera-reth`. It is for operator work: connect, run raw RPC, inspect the result, move on. No JavaScript runtime and no web3 object model.

It connects over local IPC, `http(s)`, or `ws(s)`. IPC is the primary path: if no endpoint is provided, it uses `DATADIR/reth.ipc`. Use `--exec` for one-shot calls or start the REPL for an interactive session with history and completion.

The query mini-language is intentionally small: `.count` and `.len`, `.first` and `.last`, indexed access like `.[0]` or `.[0].field`, and `.map(.field)`. Output includes list counts. Top-level arrays print `N items`; nested arrays print a path count such as `$.transactions: 3 items`.

## Quickstart

The primary workflow is local IPC against a node on the same machine.

```bash
reth-console
```

Run one command and exit over IPC:

```bash
reth-console --exec "eth.blockNumber"
```

Use a non-default data directory:

```bash
reth-console --datadir /path/to/reth
```

Use an explicit endpoint only when needed:

```bash
reth-console --exec "eth_blockNumber" http://127.0.0.1:8545
```

Example REPL session:

```text
reth> eth.getLogs [{"fromBlock":"latest","toBlock":"latest","address":"0x..."}]
reth> .count
reth> .first
```

If you are running from source, build once via Make and run the binary directly:

```bash
make all
./target/debug/reth-console
```

## CLI

```text
reth-console [endpoint]
  --datadir <path>
  --ipc-filename <name>
  --exec "<cmd>"
  --http-header key:value
  --alias alias=rpc_method
```

`--datadir` is used for default IPC resolution and the history file. `--ipc-filename` defaults to `reth.ipc`. `--http-header` and `--alias` are repeatable.

## Development

Run tests:

```bash
make test
```

Run coverage:

```bash
make test-coverage
```

Coverage writes `coverage/lcov.info`.

`make test-coverage` requires `cargo-llvm-cov` and Rust LLVM tools on the active toolchain:

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
```

If `cargo-llvm-cov` is installed into a custom Cargo home, add its `bin` directory to `PATH` before running coverage.

## Scope

This project does not try to do geth-console parity. Scope is direct RPC invocation, aliases, and compact JSON extraction.

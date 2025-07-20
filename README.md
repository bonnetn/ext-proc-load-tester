# Ext-Proc Load Tester

A load testing tool for Envoy external processing servers.

## Usage

```bash
cargo run -- <ext_proc_server_uri>
```

## Examples

```bash
# Basic load test
cargo run -- http://localhost:8080

# Custom throughput range
cargo run -- http://localhost:8080 --start-throughput 10 --end-throughput 1000

# Custom test duration
cargo run -- http://localhost:8080 --test-duration 30
``` 
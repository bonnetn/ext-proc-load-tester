# Ext-Proc Load Tester

A load testing tool for [Envoy external processing servers](https://www.envoyproxy.io/docs/envoy/latest/configuration/http/http_filters/ext_proc_filter). It opens ext_proc streams to the target server at an increasing rate (streams per second), ramping up in steps. The latency of each stream (from open to close) is recorded in JSON files.

## Usage

```bash
cargo run -- <ext_proc_server_uri>
```

This creates JSON files in the current directory, each containing the latency of every stream.

Vizualize the latencies by uploading the JSON files to https://nicolasbon.net/ext-proc-load-tester/

## Examples

```bash
# Basic load test.
cargo run -- grpc://localhost:12345

# Load test a server listening on a unix socket and write the results to a temporary directory.
cargo run -- --result-directory "$(mktemp -d)" unix:///tmp/sock

# Custom additive throughput ramp-up plan: 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000 streams per second, with each step lasting 10 seconds.
cargo run -- grpc://localhost:12345 --start-throughput 100 --end-throughput 1000 --throughput-step 100 --test-duration 10

# Custom multiplicative throughput ramp-up plan: 100, 200, 400, 800, 1600, 3200 streams per second, with each step lasting 10 seconds.
cargo run -- grpc://localhost:12345 --start-throughput 100 --end-throughput 1000 --throughput-step 0 --throughput-multiplier 2 --test-duration 10
```

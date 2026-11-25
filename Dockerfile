FROM rust:1.100-bookworm@sha256:75d26c23c24cc6ea1cdabf0714a6bff8fd7bf3248716043fd7ae86dac7f41b94 AS builder

WORKDIR /usr/src/ext-proc-load-tester

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*

COPY proto/envoy/api proto/envoy/api
COPY proto/protoc-gen-validate proto/protoc-gen-validate
COPY proto/udpa proto/udpa
COPY proto/xds proto/xds

COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src

RUN cargo install --path .

CMD ["ext-proc-load-tester", "--help"]

FROM debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ext-proc-load-tester /usr/local/bin/ext-proc-load-tester

ENTRYPOINT ["ext-proc-load-tester"]

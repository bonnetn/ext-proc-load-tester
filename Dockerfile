FROM rust:1.91-bookworm@sha256:c1e5f19e773b7878c3f7a805dd00a495e747acbdc76fb2337a4ebf0418896b33 AS builder

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

FROM debian:bookworm-slim@sha256:98f4b71de414932439ac6ac690d7060df1f27161073c5036a7553723881bffbe

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ext-proc-load-tester /usr/local/bin/ext-proc-load-tester

ENTRYPOINT ["ext-proc-load-tester"]

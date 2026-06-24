FROM rust:1.96-bookworm@sha256:19817ead3289c8c631c73df281e18b59b172f6a31f4f563290f69cddd06c30e9 AS builder

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

FROM debian:bookworm-slim@sha256:60eac759739651111db372c07be67863818726f754804b8707c90979bda511df

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ext-proc-load-tester /usr/local/bin/ext-proc-load-tester

ENTRYPOINT ["ext-proc-load-tester"]

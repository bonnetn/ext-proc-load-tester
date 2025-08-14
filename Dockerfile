FROM rust:1.89-bookworm@sha256:f1cbe94e8dbfa9deaa8a7d90d801ed81db2a84c363276b6a93055d3c4bba7409 AS builder

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

FROM debian:bookworm-slim@sha256:b1a741487078b369e78119849663d7f1a5341ef2768798f7b7406c4240f86aef

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ext-proc-load-tester /usr/local/bin/ext-proc-load-tester

ENTRYPOINT ["ext-proc-load-tester"]

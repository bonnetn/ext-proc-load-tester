FROM rust:1.99-bookworm@sha256:59037199c44290f2befcdd58dcc540164763fc296950255aaefeef096a1866b0 AS builder

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

FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241

RUN apt-get update && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/ext-proc-load-tester /usr/local/bin/ext-proc-load-tester

ENTRYPOINT ["ext-proc-load-tester"]

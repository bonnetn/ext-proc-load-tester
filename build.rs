use std::io::Result;

fn main() -> Result<()> {
    tonic_build::configure().compile_protos(
        &[
            "proto/envoy/api/envoy/service/ext_proc/v3/external_processor.proto",
        ],
        &[
            "proto/envoy/api",
            "proto/protoc-gen-validate",
            "proto/udpa",
            "proto/xds",
        ],
    )?;
    Ok(())
}

use std::io::Result;

fn main() -> Result<()> {
    tonic_build::configure().compile_protos(
        &[
            "proto/envoy/api/envoy/config/core/v3/address.proto",
            "proto/envoy/api/envoy/config/core/v3/backoff.proto",
            "proto/envoy/api/envoy/config/core/v3/base.proto",
            "proto/envoy/api/envoy/config/core/v3/extension.proto",
            "proto/envoy/api/envoy/config/core/v3/http_uri.proto",
            "proto/envoy/api/envoy/config/core/v3/socket_option.proto",
            "proto/envoy/api/envoy/extensions/filters/http/ext_proc/v3/processing_mode.proto",
            "proto/envoy/api/envoy/service/ext_proc/v3/external_processor.proto",
            "proto/envoy/api/envoy/type/v3/http_status.proto",
            "proto/envoy/api/envoy/type/v3/percent.proto",
            "proto/redirector/redirector.proto",
            "proto/protoc-gen-validate/validate/validate.proto",
        ],
        &[
            "proto/envoy/api",
            "proto/redirector",
            "proto/protoc-gen-validate",
            "proto/udpa",
        ],
    )?;
    Ok(())
}

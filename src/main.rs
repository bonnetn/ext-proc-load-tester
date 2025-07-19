mod generated;

use tonic::transport::Channel;



tonic::include_proto!("envoy.service.ext_proc.v3");

type Result<T, E=Box<dyn std::error::Error>> = std::result::Result<T, E>;

fn main() {
    println!("Hello, world!");
}

fn call_ext_proc(channel: Channel) -> Result<()> {
    let mut client = external_processor_client::ExternalProcessorClient::new(channel);
    Ok(())
}
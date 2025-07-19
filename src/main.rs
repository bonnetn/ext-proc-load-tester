mod generated;

use crate::generated::envoy::{
    config::core::v3::{HeaderMap, HeaderValue},
    service::ext_proc::v3::{
        HttpHeaders, ProcessingRequest, external_processor_client::ExternalProcessorClient,
        processing_request::Request,
    },
};
use clap::Parser;
use thiserror::Error;
use tokio::sync::mpsc::{self, error::SendError};
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};
use tonic::transport::Channel;
use tracing::trace;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The URI of the ext_proc server.
    uri: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    run(&cli).await
}

async fn run(cli: &Cli) -> Result<()> {
    println!("cli: {:?}", &cli);

    let channel = tonic::transport::Endpoint::new(cli.uri.clone())
        .map_err(Error::FailedToCreateEndpoint)?
        .connect()
        .await
        .map_err(Error::FailedToConnectToEndpoint)?;

    call_ext_proc(channel).await?;
    Ok(())
}

async fn call_ext_proc(channel: Channel) -> Result<()> {
    let mut client = ExternalProcessorClient::new(channel);
    trace!("created client");

    let (tx, mut rx) = mpsc::channel(2);
    tx.send(request_headers::create_processing_request())
        .await
        .map_err(Error::CannotSendInitialRequest)?;

    let mut request_stream = ReceiverStream::new(rx);
    trace!("created request stream");

    let response = client
        .process(request_stream)
        .await
        .map_err(Error::FailedToCallExtProc)?;
    trace!("received response");

    let mut response_stream = response.into_inner();

    let Some(processing_response) = response_stream.next().await else {
        // Early return if the stream is closed.
        trace!("stream is closed");
        return Ok(());
    };

    trace!(processing_response=?processing_response, "first processing_response received");

    let Ok(_) = tx.send(response_headers::create_processing_request()).await else {
        trace!("cannot send second request, stream is closed");
        return Ok(());
    };

    let Some(processing_response) = response_stream.next().await else {
        trace!("cannot receive second response, stream is closed");
        return Ok(());
    };

    trace!(processing_response=?processing_response, "second processing_response received");
    Ok(())
}

#[derive(Debug, Error)]
enum Error {
    #[error("failed to create gRPC endpoint: {0}")]
    FailedToCreateEndpoint(tonic::transport::Error),
    #[error("failed to connect to endpoint: {0}")]
    FailedToConnectToEndpoint(tonic::transport::Error),
    #[error("failed to call ext_proc: {0}")]
    FailedToCallExtProc(tonic::Status),
    #[error("cannot send request to ext_proc: {0}")]
    CannotSendInitialRequest(SendError<ProcessingRequest>),
}

mod request_headers {
    use super::*;

    pub(crate) fn create_processing_request() -> ProcessingRequest {
        ProcessingRequest {
            request: Some(Request::RequestHeaders(create_http_headers())),
            ..Default::default()
        }
    }

    fn create_http_headers() -> HttpHeaders {
        HttpHeaders {
            headers: Some(create_header_map()),
            ..Default::default()
        }
    }

    fn create_header_map() -> HeaderMap {
        HeaderMap {
            headers: vec![create_header_value()],
            ..Default::default()
        }
    }

    fn create_header_value() -> HeaderValue {
        HeaderValue {
            key: "test".to_string(),
            raw_value: vec![],
            ..Default::default()
        }
    }
}

mod response_headers {
    use super::*;

    pub(crate) fn create_processing_request() -> ProcessingRequest {
        ProcessingRequest {
            request: Some(Request::ResponseHeaders(create_http_headers())),
            ..Default::default()
        }
    }

    fn create_http_headers() -> HttpHeaders {
        HttpHeaders {
            headers: Some(create_header_map()),
            ..Default::default()
        }
    }

    fn create_header_map() -> HeaderMap {
        HeaderMap {
            headers: vec![create_header_value()],
            ..Default::default()
        }
    }

    fn create_header_value() -> HeaderValue {
        HeaderValue {
            key: "test".to_string(),
            raw_value: vec![],
            ..Default::default()
        }
    }
}

use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::transport::Channel;

use crate::{
    app::{
        error::{Error, Result},
        sample_requests::{request_headers, response_headers},
    },
    generated::envoy::service::ext_proc::v3::external_processor_client::ExternalProcessorClient,
};

#[allow(dead_code)]
pub(crate) trait Worker {
    fn run(&self) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct GrpcWorker {
    channel: Channel,
}

impl GrpcWorker {
    #[allow(dead_code)]
    pub(crate) fn new(channel: &Channel) -> Self {
        Self {
            channel: channel.clone(),
        }
    }
}

impl Worker for GrpcWorker {
    async fn run(&self) -> Result<()> {
        let mut client = ExternalProcessorClient::new(self.channel.clone());

        let (tx, rx) = mpsc::channel(2);
        tx.send(request_headers::create_processing_request())
            .await
            .map_err(|e| Error::CannotSendInitialRequest(Box::new(e)))?;

        let request_stream = ReceiverStream::new(rx);

        let response = client
            .process(request_stream)
            .await
            .map_err(|e| Error::FailedToCallExtProc(Box::new(e)))?;

        let mut response_stream = response.into_inner();

        let Some(_processing_response) = response_stream.next().await else {
            // Early return if the stream is closed.
            return Ok(());
        };

        let Ok(()) = tx.send(response_headers::create_processing_request()).await else {
            return Ok(());
        };

        let Some(_processing_response) = response_stream.next().await else {
            return Ok(());
        };

        Ok(())
    }
}

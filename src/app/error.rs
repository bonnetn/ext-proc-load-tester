use std::num::TryFromIntError;

use crate::generated::envoy::service::ext_proc::v3::ProcessingRequest;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub(crate) enum Error {
    #[error("failed to create gRPC endpoint: {0}")]
    FailedToCreateEndpoint(tonic::transport::Error),
    #[error("failed to connect to endpoint: {0}")]
    FailedToConnectToEndpoint(tonic::transport::Error),
    #[error("failed to call ext_proc: {0}")]
    FailedToCallExtProc(Box<tonic::Status>),
    #[error("cannot send request to ext_proc: {0}")]
    CannotSendInitialRequest(Box<SendError<ProcessingRequest>>),
    #[error(
        "could not reach target throughput {0} req/s, actual throughput {1} req/s ({2}% of target). This indicates that the LOAD TESTER was saturated."
    )]
    CouldNotReachTargetThroughput(u64, u64, u64),
    #[error("failed to write report: {0}")]
    WriteReport(std::io::Error),
    #[error("estimated request count is too large: {0}")]
    EstimatedRequestCountTooLarge(TryFromIntError),
    #[error("selected parameters would result in too many throughputs being tested")]
    TooManyThroughputsToTest,
}

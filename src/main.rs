#![deny(clippy::correctness)]

#![warn(
    clippy::suspicious,
    clippy::complexity,
    clippy::perf,
    clippy::style,
    clippy::pedantic,
    clippy::cargo,
)]

#![allow(
    clippy::restriction,
)]

#![feature(custom_inner_attributes)]
#![clippy::msrv = "1.88.0"]

mod generated;

use std::{
    env, io::Write, path::{Path, PathBuf}, time::Duration, future::Future
};

use crate::generated::envoy::{
    service::ext_proc::v3::{
        ProcessingRequest, external_processor_client::ExternalProcessorClient,
    },
};
use clap::Parser;
use futures::stream::FuturesUnordered;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use thiserror::Error;
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    time::Instant,
};
use tokio::{
    select,
    sync::mpsc::{self, error::SendError},
};
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};
use tonic::transport::Channel;
use zstd::Encoder;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The URI of the `ext_proc` server.
    uri: String,

    /// The duration of each throughput level in seconds.
    #[arg(long, default_value = "10", value_parser = validate_test_duration_seconds)]
    test_duration: Duration,

    /// The minimum throughput (requests per second) to use.
    #[arg(long, default_value_t = 1.0, value_parser = validate_start_throughput)]
    start_throughput: f32,

    /// The maximum throughput (requests per second) to use.
    #[arg(long, default_value_t = 16378.0, value_parser = validate_end_throughput)]
    end_throughput: f32,

    /// The multiplier for the next throughput level.
    #[arg(long, default_value_t = 1.0, value_parser = validate_throughput_multiplier)]
    throughput_multiplier: f32,

    /// The number of requests added to the next throughput level.
    #[arg(long, default_value_t = 0.0, value_parser = validate_throughput_step)]
    throughput_step: f32,

    /// The directory to write the results to.
    /// Defaults to the current working directory.
    #[arg(long, value_parser = validate_result_directory)]
    result_directory: Option<PathBuf>,
}

fn validate_test_duration_seconds(v: &str) -> Result<Duration, String> {
    let v: u64 = v
        .parse()
        .map_err(|_| format!("test duration must be a number, got {v}"))?;
    if v == 0 {
        return Err(format!(
            "test duration must be strictly positive, got {v}"
        ));
    }

    Ok(Duration::from_secs(v))
}

fn validate_start_throughput(v: &str) -> Result<f32, String> {
    let v: f32 = v
        .parse()
        .map_err(|_| format!("start throughput must be a number, got {v}"))?;
    if !v.is_finite() {
        return Err(format!("start throughput must be finite, got {v}"));
    }

    if v <= 0. {
        return Err(format!(
            "start throughput must be strictly positive, got {v}"
        ));
    }

    Ok(v)
}

fn validate_end_throughput(v: &str) -> Result<f32, String> {
    let v: f32 = v
        .parse()
        .map_err(|_| format!("end throughput must be a number, got {v}"))?;
    if !v.is_finite() {
        return Err(format!("end throughput must be finite, got {v}"));
    }

    if v <= 0. {
        return Err(format!(
            "end throughput must be strictly positive, got {v}"
        ));
    }

    Ok(v)
}

fn validate_throughput_step(v: &str) -> Result<f32, String> {
    let v: f32 = v
        .parse()
        .map_err(|_| format!("throughput step must be a number, got {v}"))?;
    if !v.is_finite() {
        return Err(format!("throughput step must be finite, got {v}"));
    }

    if v < 0. {
        return Err(format!(
            "throughput step must be above or equal to 0, got {v}"
        ));
    }

    Ok(v)
}

fn validate_throughput_multiplier(v: &str) -> Result<f32, String> {
    let v: f32 = v
        .parse()
        .map_err(|_| format!("throughput multiplier must be a number, got {v}"))?;
    if !v.is_finite() {
        return Err(format!("throughput multiplier must be finite, got {v}"));
    }

    if v < 1. {
        return Err(format!(
            "throughput multiplier must be above or equal to 1, got {v}"
        ));
    }

    Ok(v)
}

fn validate_result_directory(v: &str) -> Result<PathBuf, String> {
    let v: PathBuf = v
        .parse()
        .map_err(|_| format!("result directory must be a path, got {v}"))?;

    if !v.is_dir() {
        return Err("result directory is not a directory".to_string());
    }

    Ok(v)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let channel = tonic::transport::Endpoint::new(cli.uri.clone())
        .map_err(Error::FailedToCreateEndpoint)?
        .connect()
        .await
        .map_err(Error::FailedToConnectToEndpoint)?;

    let worker = || async {
        let start = Instant::now();
        call_ext_proc(channel.clone()).await?;
        Ok(start.elapsed())
    };

    let result_directory = match &cli.result_directory {
        Some(dir) => Path::new(dir),
        None => &env::current_dir().expect("current directory can be read"),
    };

    run(&cli, &worker, result_directory).await
}

async fn run<Fut>(cli: &Cli, run_worker: &impl Fn() -> Fut, result_directory: &Path) -> Result<()>
where
    Fut: Future<Output = Result<Duration>> + Send,
{
    let throughputs = get_all_throughputs(cli);
    let multi_progress = MultiProgress::new();
    let progress_style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");

    let mut progress_bars = vec![];
    for throughput in &throughputs {
        let estimated_request_count = cli.test_duration.as_secs_f32() * *throughput;

        let pb = multi_progress.add(ProgressBar::new(estimated_request_count as u64));
        pb.set_style(progress_style.clone());
        pb.set_message(format!("{throughput} req/s"));
        progress_bars.push(pb);
    }

    for (throughput, pb) in throughputs.into_iter().zip(progress_bars.into_iter()) {
        let deadline = tokio::time::Instant::now() + cli.test_duration;
        run_with_throughput(
            &pb,
            cli,
            throughput,
            deadline,
            run_worker,
            result_directory,
        )
        .await?;
        pb.finish();
    }

    Ok(())
}

fn get_all_throughputs(cli: &Cli) -> Vec<f32> {
    let from: f32 = cli.start_throughput;
    let step: f32 = cli.throughput_step;
    let mul: f32 = cli.throughput_multiplier;

    let mut throughputs = vec![];
    let mut i = 0_usize;
    loop {
        let throughput = if (mul - 1.).abs() < f32::EPSILON {
            step.mul_add(i as f32, from)
        } else {
            let r = step / (1. - mul);
            mul.powf(i as f32).mul_add(from - r, r)
        };

        if throughput > cli.end_throughput {
            break;
        }

        throughputs.push(throughput);
        i += 1;
    }

    throughputs
}

async fn run_with_throughput<Fut>(
    pb: &ProgressBar,
    cli: &Cli,
    target_throughput: f32,
    deadline: tokio::time::Instant,
    run_worker: impl Fn() -> Fut,
    result_directory: &Path,
) -> Result<()>
where
    Fut: Future<Output = Result<Duration>> + Send,
{
    let mut timer = tokio::time::interval(Duration::from_secs_f32(1. / target_throughput));
    let mut f: FuturesUnordered<Fut> = FuturesUnordered::new();
    let mut requests_sent = 0;

    let estimated_count = (cli.test_duration.as_secs_f32() * target_throughput * 1.1) as usize;
    let mut durations = Vec::with_capacity(estimated_count);

    loop {
        select! {
            _ = timer.tick() => {
                f.push(run_worker());
                pb.inc(1);
                requests_sent += 1;
            }

            result = f.next() => {
                match result {
                    Some(result) => {
                        durations.push(result?);
                    }
                    None => {
                    select! {
                        _ = timer.tick() => {
                            f.push(run_worker());
                            pb.inc(1);
                            requests_sent += 1;
                        }
                        _ = tokio::time::sleep_until(deadline) => {
                            break;
                        }
                    }
                    }
                }
            }

            _ = tokio::time::sleep_until(deadline) => {
                while (f.next().await).is_some() {
                }
                break;
            }
        }
    }

    let actual_throughput = requests_sent as f32 / cli.test_duration.as_secs_f32();
    let percent_of_target_throughput = 100. * actual_throughput / target_throughput;
    if percent_of_target_throughput < 90. {
        return Err(Error::CouldNotReachTargetThroughput(
            target_throughput,
            actual_throughput,
            percent_of_target_throughput,
        ));
    }

    write_report(result_directory, target_throughput, &durations)
        .await
        .map_err(Error::WriteReport)?;

    let avg_duration = durations.iter().sum::<Duration>() / u32::try_from(durations.len()).unwrap_or(1);
    let min_duration = durations.iter().min().unwrap();
    let max_duration = durations.iter().max().unwrap();

    pb.finish_with_message(format!(
        "{} req/s: {} requests, avg: {:?}, min: {:?}, max: {:?}",
        target_throughput,
        durations.len(),
        avg_duration,
        min_duration,
        max_duration,
    ));

    Ok(())
}

async fn write_report(
    directory_path: &Path,
    target_throughput: f32,
    durations: &[Duration],
) -> Result<(), std::io::Error> {
    let file_name = format!("durations_{}.json.txt.zst", target_throughput.floor() as u32);
    let file_path = directory_path.join(file_name);

    let mut v = Vec::new();
    let mut encoder = Encoder::new(&mut v, 0)?;

    encoder.write_all(b"[")?;

    let mut has_previous_value = false;
    for duration in durations {
        if has_previous_value {
            encoder.write_all(b",")?;
        }
        encoder.write_all(format!("{}", duration.as_nanos()).as_bytes())?;
        has_previous_value = true;
    }

    encoder.write_all(b"]")?;
    encoder.finish()?;

    let mut f = File::create(file_path).await?;
    f.write_all(&v).await?;

    Ok(())
}

async fn call_ext_proc(channel: Channel) -> Result<()> {
    let mut client = ExternalProcessorClient::new(channel);

    let (tx, rx) = mpsc::channel(2);
    tx.send(request_headers::create_processing_request())
        .await
        .map_err(Error::CannotSendInitialRequest)?;

    let request_stream = ReceiverStream::new(rx);

    let response = client
        .process(request_stream)
        .await
        .map_err(Error::FailedToCallExtProc)?;

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
    #[error(
        "could not reach target throughput {0} req/s, actual throughput {1} req/s ({2}% of target). This indicates that the LOAD TESTER was saturated."
    )]
    CouldNotReachTargetThroughput(f32, f32, f32),
    #[error("failed to write report: {0}")]
    WriteReport(std::io::Error),
}

mod request_headers {
    use crate::generated::envoy::{
        config::core::v3::{HeaderMap, HeaderValue},
        service::ext_proc::v3::{HttpHeaders, ProcessingRequest, processing_request::Request},
    };

    pub fn create_processing_request() -> ProcessingRequest {
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
    use crate::generated::envoy::{
        config::core::v3::{HeaderMap, HeaderValue},
        service::ext_proc::v3::{HttpHeaders, ProcessingRequest, processing_request::Request},
    };

    pub fn create_processing_request() -> ProcessingRequest {
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

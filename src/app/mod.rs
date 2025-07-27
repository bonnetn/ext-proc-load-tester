use std::{
    env,
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};

use crate::{
    app::{
        cli::Cli,
        error::Error,
        sample_requests::{request_headers, response_headers},
    },
    generated::envoy::service::ext_proc::v3::{
        ProcessingRequest, external_processor_client::ExternalProcessorClient,
    },
};
use clap::Parser;
use futures::stream::FuturesUnordered;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::{select, sync::mpsc, time::MissedTickBehavior};
use tokio_stream::{StreamExt as _, wrappers::ReceiverStream};
use tonic::transport::Channel;

mod cli;
pub(crate) mod error;
mod fixture;
mod report;
mod sample_requests;

use error::Result;

// If we managed to send 95% of the expected requests, we consider the test successful.
const ACCEPTABLE_PERCENTAGE_OF_TARGET_THROUGHPUT: u64 = 95;

pub(crate) async fn run() -> Result<()> {
    let cli = Cli::parse();

    let channel = tonic::transport::Endpoint::new(cli.uri.clone())
        .map_err(Error::FailedToCreateEndpoint)?
        .connect()
        .await
        .map_err(Error::FailedToConnectToEndpoint)?;

    let worker = async || {
        let start = Instant::now();
        call_ext_proc(channel.clone()).await?;
        Ok(start.elapsed())
    };

    let result_directory = match &cli.result_directory {
        Some(dir) => Path::new(dir),
        None => &env::current_dir().expect("current directory can be read"),
    };

    let _request_fixture = match &cli.request_fixture_path {
        Some(path) => {
            let file = File::open(path).map_err(Error::FailedToOpenRequestFixture)?;
            let reader = BufReader::new(file);
            let dto: fixture::json::ProcessingRequest =
                serde_json::from_reader(reader).map_err(Error::FailedToParseRequestFixture)?;
            let request_fixture: ProcessingRequest = dto.try_into()?;
            request_fixture
        }
        None => todo!(""),
    };

    load_test(&cli, &worker, result_directory).await
}

async fn load_test<Fut>(
    cli: &Cli,
    run_worker: &impl Fn() -> Fut,
    result_directory: &Path,
) -> Result<()>
where
    Fut: Future<Output = Result<Duration>> + Send,
{
    let throughputs = get_all_throughputs(cli)?;
    let multi_progress = MultiProgress::new();
    let progress_style = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");

    let mut progress_bars = vec![];
    for throughput in &throughputs {
        let estimated_request_count = cli.test_duration.as_secs() * *throughput;

        let pb = multi_progress.add(ProgressBar::new(estimated_request_count));
        pb.set_style(progress_style.clone());
        pb.set_message(format!("{throughput} req/s"));
        progress_bars.push(pb);
    }

    for (throughput, pb) in throughputs.into_iter().zip(progress_bars.into_iter()) {
        let deadline = Instant::now() + cli.test_duration;
        run_with_throughput(&pb, cli, throughput, deadline, run_worker, result_directory).await?;
        pb.finish();
    }

    Ok(())
}

fn get_all_throughputs(cli: &Cli) -> Result<Vec<u64>> {
    let u0 = cli.start_throughput;
    let b = cli.throughput_step;
    let a = cli.throughput_multiplier;

    let mut throughputs = vec![];
    let mut value = u0;
    while value <= cli.end_throughput {
        throughputs.push(value);
        if a != 1 {
            value *= a;
        }
        value += b;
        if throughputs.len() > 100 {
            return Err(Error::TooManyThroughputsToTest);
        }
    }

    Ok(throughputs)
}

async fn run_with_throughput<Fut>(
    pb: &ProgressBar,
    cli: &Cli,
    target_throughput: u64,
    deadline: Instant,
    run_worker: impl Fn() -> Fut,
    result_directory: &Path,
) -> Result<()>
where
    Fut: Future<Output = Result<Duration>> + Send,
{
    let interval_ns = 1_000_000_000 / target_throughput;
    let mut timer = tokio::time::interval(Duration::from_nanos(interval_ns));

    // NOTE: If the load tester is saturated, it will not be able to keep up with the
    // interval. We measure at the end of the test, the number of requests ACTUALLY sent
    // vs the number of requests that were expected to be sent.
    // Skipping the missed ticks allows us to get a signal on the saturation of the load tester.
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut f: FuturesUnordered<Fut> = FuturesUnordered::new();
    let mut requests_sent = 0;

    let target_request_count = cli.test_duration.as_secs() * target_throughput;

    let target_request_count_usize: usize = target_request_count
        .try_into()
        .map_err(Error::EstimatedRequestCountTooLarge)?;

    let mut durations = Vec::with_capacity(target_request_count_usize);

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
                        () = tokio::time::sleep_until(deadline.into()) => {
                            // NOTE: No ongoing worker, we can just break.
                            break;
                        }
                    }
                    }
                }
            }

            () = tokio::time::sleep_until(deadline.into()) => {
                while (f.next().await).is_some() {
                    // NOTE: We are waiting for the workers to finish.
                }
                break;
            }
        }
    }

    let actual_throughput = requests_sent / cli.test_duration.as_secs();
    let percent_of_target_throughput = 100 * requests_sent / target_request_count;

    if percent_of_target_throughput < ACCEPTABLE_PERCENTAGE_OF_TARGET_THROUGHPUT {
        return Err(Error::CouldNotReachTargetThroughput(
            target_throughput,
            actual_throughput,
            percent_of_target_throughput,
        ));
    }

    report::write(result_directory, target_throughput, &durations)
        .await
        .map_err(Error::WriteReport)?;

    let avg_duration =
        durations.iter().sum::<Duration>() / u32::try_from(durations.len()).unwrap_or(1);
    let min_duration = durations.iter().min().unwrap();
    let max_duration = durations.iter().max().unwrap();

    pb.finish_with_message(format!(
        "{target_throughput} req/s: {percent_of_target_throughput}% of planned requests sent, avg: {avg_duration:?}, min: {min_duration:?}, max: {max_duration:?}",
    ));

    Ok(())
}

async fn call_ext_proc(channel: Channel) -> Result<()> {
    let mut client = ExternalProcessorClient::new(channel);

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

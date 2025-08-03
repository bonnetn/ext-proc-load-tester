use std::{env, path::Path, time::Duration};

use crate::app::{
    cli::Cli,
    error::Error,
    scheduler::{REPORT_INTERVAL, Scheduler},
    worker::GrpcWorker,
};
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::runtime::Handle;

mod cli;
pub(crate) mod error;
mod report;
mod sample_requests;
mod scheduler;
mod worker;

use error::Result;

// If we managed to send 95% of the expected requests, we consider the test successful.
const ACCEPTABLE_PERCENTAGE_OF_TARGET_THROUGHPUT: u64 = 95;

pub(crate) async fn run() -> Result<()> {
    let cli = Cli::parse();

    let concurrency = Handle::current().metrics().num_workers();
    let mut workers = vec![];

    for _ in 0..concurrency {
        let channel = tonic::transport::Endpoint::new(cli.uri.clone())
            .map_err(Error::FailedToCreateEndpoint)?
            .connect()
            .await
            .map_err(Error::FailedToConnectToEndpoint)?;

        let worker = GrpcWorker::new(&channel);
        workers.push(worker);
    }

    let mut scheduler = Scheduler::new(&workers, REPORT_INTERVAL)?;

    let result_directory = match &cli.result_directory {
        Some(dir) => Path::new(dir),
        None => &env::current_dir().expect("current directory can be read"),
    };

    load_test(&cli, &mut scheduler, result_directory).await
}

async fn load_test(
    cli: &Cli,
    scheduler: &mut Scheduler<GrpcWorker>,
    result_directory: &Path,
) -> Result<()> {
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
        run_with_throughput(&pb, cli, throughput, scheduler, result_directory).await?;
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

async fn run_with_throughput(
    pb: &ProgressBar,
    cli: &Cli,
    target_throughput: u64,
    scheduler: &mut Scheduler<GrpcWorker>,
    result_directory: &Path,
) -> Result<()> {
    let interval = Duration::from_secs(1)
        .checked_div(target_throughput.try_into().unwrap()) // TODO: Make target throughput u32
        .expect("target throughput must not be 0");
    let timeout = cli.test_duration;

    let target_request_count = cli.test_duration.as_secs() * target_throughput;

    let results = scheduler.run(interval, timeout, pb).await?;

    let request_sent = results.iter().map(|r| r.request_sent).sum::<u64>();
    let durations = results
        .into_iter()
        .flat_map(|r| r.durations)
        .collect::<Vec<_>>();

    let actual_throughput = request_sent / cli.test_duration.as_secs();
    let percent_of_target_throughput = 100 * request_sent / target_request_count;

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

use std::{path::PathBuf, time::Duration};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    /// The URI of the `ext_proc` server.
    pub(crate) uri: String,

    /// The duration of each throughput level in seconds.
    #[arg(long, default_value = "10", value_parser = validate_test_duration_seconds)]
    pub(crate) test_duration: Duration,

    /// The minimum throughput (requests per second) to use.
    #[arg(long, default_value_t = 1, value_parser = validate_start_throughput)]
    pub(crate) start_throughput: u64,

    /// The maximum throughput (requests per second) to use.
    #[arg(long, default_value_t = 16378, value_parser = validate_end_throughput)]
    pub(crate) end_throughput: u64,

    /// The multiplier for the next throughput level.
    #[arg(long, default_value_t = 1, value_parser = validate_throughput_multiplier)]
    pub(crate) throughput_multiplier: u64,

    /// The number of requests added to the next throughput level.
    #[arg(long, default_value_t = 0, value_parser = validate_throughput_step)]
    pub(crate) throughput_step: u64,

    /// The directory to write the results to.
    /// Defaults to the current working directory.
    #[arg(long, value_parser = validate_result_directory)]
    pub(crate) result_directory: Option<PathBuf>,
}

fn validate_test_duration_seconds(v: &str) -> Result<Duration, String> {
    let v: u64 = v
        .parse()
        .map_err(|_| format!("test duration must be a integer (seconds), got {v}"))?;

    Ok(Duration::from_secs(v))
}

fn validate_start_throughput(v: &str) -> Result<u64, String> {
    let v: u64 = v.parse().map_err(|_| {
        format!("start throughput must be a integer (requests per second), got {v}")
    })?;

    if v < 1 {
        return Err(format!(
            "start throughput must be strictly positive, got {v}"
        ));
    }

    Ok(v)
}

fn validate_end_throughput(v: &str) -> Result<u64, String> {
    let v: u64 = v
        .parse()
        .map_err(|_| format!("end throughput must be a integer (requests per second), got {v}"))?;

    if v < 1 {
        return Err(format!("end throughput must be strictly positive, got {v}"));
    }

    Ok(v)
}

fn validate_throughput_step(v: &str) -> Result<u64, String> {
    let v: u64 = v.parse().map_err(|_| {
        format!("throughput step must be a integer (requests per second per run), got {v}")
    })?;

    Ok(v)
}

fn validate_throughput_multiplier(v: &str) -> Result<u64, String> {
    let v: u64 = v.parse().map_err(|_| {
        format!("throughput multiplier must be a integer (multiplier per run), got {v}")
    })?;

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

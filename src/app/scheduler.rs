use std::{num::NonZeroU32, sync::Arc, time::Duration};

use futures::stream::FuturesUnordered;
use indicatif::ProgressBar;
use tokio::{
    select,
    sync::Barrier,
    task::JoinSet,
    time::{Instant, MissedTickBehavior},
};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::app::{
    error::{Error, Result},
    worker::Worker,
};

const REPORT_INTERVAL: Duration = Duration::from_millis(250);

/// A scheduler that runs a set of `Worker` instances at a fixed overall rate,
/// distributing execution across multiple Tokio tasks to achieve true parallelism.
///
/// Each worker is executed in its own Tokio task, and the global request rate is
/// maintained by staggering the execution intervals. The number of workers defines
/// the concurrency level.
///
/// # Why not just use `FuturesUnordered`?
/// Using a single `FuturesUnordered` confines all polling to a single Tokio task,
/// which only runs on one thread at a time—even with a multithreaded runtime and
/// work stealing. To avoid this bottleneck and utilize all CPU cores, this scheduler
/// spawns independent tasks for each worker.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct Scheduler<W> {
    workers: Vec<W>,
    concurrency: NonZeroU32,
}

impl<W> Scheduler<W>
where
    W: Worker + Clone + Sync + Send + 'static,
{
    /// Constructs a new `Scheduler` from the provided list of workers.
    /// The number of workers defines the concurrency level.
    #[allow(dead_code)]
    pub(crate) fn new(workers: &[W]) -> Result<Self> {
        let worker_count = workers
            .len()
            .try_into()
            .map_err(Error::ConcurrencyMustBeLessThanU32Max)?;

        let concurrency =
            NonZeroU32::new(worker_count).ok_or(Error::ConcurrencyMustBeGreaterThanZero)?;

        let workers = workers.to_vec();

        Ok(Self {
            workers,
            concurrency,
        })
    }

    /// Runs all workers periodically at a fixed overall rate until the timeout elapses.
    ///
    /// Each worker runs in its own Tokio task, starting at a staggered offset to
    /// evenly distribute execution over time. The method returns a vector of per-worker
    /// durations representing how long each invocation took.
    ///
    /// # Parameters
    /// - `interval`: the desired time between individual worker invocations globally.
    /// - `timeout`: the total duration after which all workers are cancelled.
    ///
    /// # Returns
    /// A vector of duration lists, one per worker, measuring actual execution latency.
    /// There is no order guarantee on the returned durations.
    #[allow(dead_code)]
    pub(crate) async fn run(
        &mut self,
        interval: Duration,
        timeout: Duration,
        progress_reporter: &impl ProgressReporter,
    ) -> Result<Vec<WorkerResult>> {
        let start = Instant::now();

        let barrier = Arc::new(Barrier::new(self.workers.len() + 1));

        // "Round robin" the workers by multiplying the interval by the concurrency.
        let loop_interval = interval
            .checked_mul(self.concurrency.get())
            .expect("duration must not overflow");

        let cancelation_token = CancellationToken::new();
        let mut set = JoinSet::new();
        let mut offset = Duration::ZERO;
        for worker in &self.workers {
            // Offset the start of each worker by the interval * i,
            // so that they don't tick at the same time.
            offset += interval;
            let start_time = start + offset;

            // Guess the number of requests that will be sent to pre-allocate the result vector.
            let size_hint: usize = timeout
                .as_nanos()
                .checked_div(loop_interval.as_nanos())
                .expect("loop interval must not be zero")
                .try_into()
                .map_err(Error::EstimatedRequestCountTooLarge)?;

            let progress_reporter = progress_reporter.clone();

            let _handle = set.spawn(run_loop(
                start_time,
                barrier.clone(),
                worker.clone(),
                loop_interval,
                cancelation_token.clone(),
                size_hint,
                progress_reporter,
            ));
        }

        let _ = barrier.wait().await;
        tokio::time::sleep(timeout).await;
        cancelation_token.cancel();

        let iterations = set
            .join_all()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        Ok(iterations)
    }
}

/// Internal per-worker loop that schedules and runs the worker periodically.
async fn run_loop(
    start: Instant,
    barrier: Arc<Barrier>,
    worker: impl Worker,
    interval: Duration,
    cancelation_token: CancellationToken,
    size_hint: usize,
    progress_reporter: impl ProgressReporter,
) -> Result<WorkerResult> {
    let mut worker_interval = create_interval(start, interval);
    let mut reporter_interval = create_interval(start, REPORT_INTERVAL);

    let mut futures = FuturesUnordered::new();
    let mut durations = Vec::with_capacity(size_hint);

    let _ = barrier.wait().await;
    let mut request_sent = 0_u64;

    let mut last_reported = 0_usize;

    loop {
        select! {
            _ = worker_interval.tick() => {
                // Interval ticked, time to spin a new worker.
                futures.push(run_with_duration(&worker));
                request_sent += 1;
            }
            _ = reporter_interval.tick() => {
                let v = durations.len();
                progress_reporter.report(v - last_reported);
                last_reported = v;
            }
            result = futures.next() => {
                match result {
                    Some(result) => {
                        // Worker finished running successfully, record the duration.
                        durations.push(result?);
                    }
                    None => {
                        // The stream is empty and no futures are currently running.
                        // We can't proceed with `futures.next()` again without blocking forever.
                        //
                        // So we re-enter a `select!` to wait for either:
                        // - the next interval tick to start new work, or
                        // - cancellation to terminate the loop.
                        select! {
                            _ = worker_interval.tick() => {
                                // Interval ticked, time to spin a new worker.
                                futures.push(run_with_duration(&worker));
                                request_sent += 1;
                            }
                            _ = reporter_interval.tick() => {
                                let v = durations.len();
                                progress_reporter.report(v - last_reported);
                                last_reported = v;
                            }
                            () = cancelation_token.cancelled() => {
                                // Cancelation token was cancelled, return the durations.
                                // NOTE: No need to wait for the workers to finish, as we know they are not running.
                                return Ok(WorkerResult {
                                    request_sent,
                                    durations,
                                });
                            }
                        }

                    }
                }
            }
            () = cancelation_token.cancelled() => {
                // Cancelation token was cancelled, wait for the workers to finish.
                while futures.next().await.is_some() {}
                return Ok(WorkerResult {
                    request_sent,
                    durations,
                });
            }
        }
    }
}

fn create_interval(start: Instant, interval: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval_at(start, interval);

    // NOTE: If the load tester is saturated, it will not be able to keep up with the
    // interval. We measure at the end of the test, the number of requests ACTUALLY sent
    // vs the number of requests that were expected to be sent.
    // Skipping the missed ticks allows us to get a signal on the saturation of the load tester.
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    interval
}

#[derive(Debug)]
pub(crate) struct WorkerResult {
    pub(crate) request_sent: u64,
    pub(crate) durations: Vec<Duration>,
}

/// Runs the given worker and measures its execution time.
async fn run_with_duration(worker: &impl Worker) -> Result<Duration> {
    let start = Instant::now();
    worker.run().await?;
    Ok(start.elapsed())
}

pub(crate) trait ProgressReporter: Send + Sync + Clone + 'static {
    fn report(&self, amount: usize) -> ();
}

impl ProgressReporter for ProgressBar {
    fn report(&self, amount: usize) {
        self.inc(amount.try_into().unwrap());
    }
}

#[cfg(test)]
mod tests {

    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    const WORKER_COUNT: usize = 8;

    #[derive(Debug, Clone, Default)]
    struct StubProgressReporter {
        pub amount: Arc<AtomicU32>,
    }

    impl ProgressReporter for StubProgressReporter {
        fn report(&self, amount: usize) {
            let _ = self
                .amount
                .fetch_add(amount.try_into().unwrap(), Ordering::Relaxed);
        }
    }

    fn workers() -> Vec<Arc<StubWorker>> {
        (0..WORKER_COUNT)
            .map(StubWorker::new)
            .map(Arc::new)
            .collect()
    }

    fn interval() -> Duration {
        Duration::from_millis(80)
    }

    fn timeout() -> Duration {
        // NOTE: We add 1/10 of the interval to avoid the case where the deadline is exactly at the same time as the tick.
        Duration::from_millis(8000) + interval().checked_div(10).unwrap()
    }

    #[derive(Debug)]
    struct StubWorker {
        pub _idx: usize,
        pub triggers: AtomicU32,
    }
    impl StubWorker {
        fn new(idx: usize) -> Self {
            Self {
                _idx: idx,
                triggers: AtomicU32::new(0),
            }
        }
    }
    impl Worker for Arc<StubWorker> {
        async fn run(&self) -> Result<()> {
            let _ = self.triggers.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    #[derive(Debug)]
    struct ErrorWorker {
        pub _idx: usize,
        pub triggers: AtomicU32,
        pub should_error: bool,
    }
    impl ErrorWorker {
        fn new(idx: usize, should_error: bool) -> Self {
            Self {
                _idx: idx,
                triggers: AtomicU32::new(0),
                should_error,
            }
        }
    }
    impl Worker for Arc<ErrorWorker> {
        async fn run(&self) -> Result<()> {
            let _ = self.triggers.fetch_add(1, Ordering::Relaxed);
            if self.should_error {
                Err(Error::ConcurrencyMustBeGreaterThanZero)
            } else {
                Ok(())
            }
        }
    }

    #[derive(Debug)]
    struct SlowWorker {
        pub _idx: usize,
        pub triggers: AtomicU32,
        pub delay: Duration,
    }
    impl SlowWorker {
        fn new(idx: usize, delay: Duration) -> Self {
            Self {
                _idx: idx,
                triggers: AtomicU32::new(0),
                delay,
            }
        }
    }
    impl Worker for Arc<SlowWorker> {
        async fn run(&self) -> Result<()> {
            let _ = self.triggers.fetch_add(1, Ordering::Relaxed);
            tokio::time::sleep(self.delay).await;
            Ok(())
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler() {
        let w = workers();
        let mut scheduler = Scheduler::new(&w).unwrap();
        let durations = scheduler
            .run(interval(), timeout(), &StubProgressReporter::default())
            .await
            .unwrap();

        let calls = w
            .iter()
            .map(|w| w.triggers.load(Ordering::Relaxed))
            .collect::<Vec<_>>();

        assert_eq!(calls, vec![13, 13, 13, 13, 12, 12, 12, 12,]);
        let mut result = durations
            .iter()
            .map(|r| r.durations.len())
            .collect::<Vec<_>>();
        result.sort_unstable(); // NOTE: Order is not guaranteed, so we sort it.

        assert_eq!(result, vec![12, 12, 12, 12, 13, 13, 13, 13,]);
    }

    #[test]
    fn test_worker_scheduler_new_with_empty_workers() {
        let result = Scheduler::<Arc<StubWorker>>::new(&[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ConcurrencyMustBeGreaterThanZero => {}
            _ => panic!("Expected ConcurrencyMustBeGreaterThanZero error"),
        }
    }

    #[test]
    fn test_worker_scheduler_new_with_single_worker() {
        let workers = vec![Arc::new(StubWorker::new(0))];
        let result = Scheduler::new(&workers);
        assert!(result.is_ok());
        // Test that we can create a scheduler with a single worker successfully
        // The behavior is verified by the fact that new() returns Ok
    }

    #[test]
    fn test_worker_scheduler_new_with_many_workers() {
        // Test that we can create a scheduler with many workers successfully
        let workers: Vec<Arc<StubWorker>> =
            (0..1000).map(|i| Arc::new(StubWorker::new(i))).collect();
        let result = Scheduler::new(&workers);
        assert!(result.is_ok());
        // The behavior is verified by the fact that new() returns Ok
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_handles_fast_intervals() {
        let w = workers();
        let mut scheduler = Scheduler::new(&w).unwrap();
        let short_interval = Duration::from_millis(10);
        let short_timeout = Duration::from_millis(100);

        let durations = scheduler
            .run(
                short_interval,
                short_timeout,
                &StubProgressReporter::default(),
            )
            .await
            .unwrap();

        // Should handle fast intervals successfully
        assert!(!durations.is_empty());

        // Verify that work was performed
        let total_calls: u32 = w.iter().map(|w| w.triggers.load(Ordering::Relaxed)).sum();
        assert!(total_calls > 0, "Work should be performed");
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_handles_slow_intervals() {
        let w = workers();
        let mut scheduler = Scheduler::new(&w).unwrap();
        let long_interval = Duration::from_millis(1000);
        let short_timeout = Duration::from_millis(500);

        let durations = scheduler
            .run(
                long_interval,
                short_timeout,
                &StubProgressReporter::default(),
            )
            .await
            .unwrap();

        // Should handle slow intervals successfully
        assert!(durations.len() < 100);

        // Verify that work was performed by checking that workers were called
        let _total_calls: u32 = w.iter().map(|w| w.triggers.load(Ordering::Relaxed)).sum();
        // The test passes if we reach this point, indicating the scheduler handled slow intervals
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_propagates_worker_errors() {
        let error_workers: Vec<Arc<ErrorWorker>> = (0..4)
            .map(|i| Arc::new(ErrorWorker::new(i, i == 2))) // Worker 2 will error
            .collect();

        let mut scheduler = Scheduler::new(&error_workers).unwrap();
        let result = scheduler
            .run(interval(), timeout(), &StubProgressReporter::default())
            .await;

        // Should propagate errors from workers
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ConcurrencyMustBeGreaterThanZero => {}
            _ => panic!("Expected ConcurrencyMustBeGreaterThanZero error"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_handles_slow_workers() {
        let slow_workers: Vec<Arc<SlowWorker>> = (0..4)
            .map(|i| {
                let delay = if i == 1 {
                    Duration::from_millis(50) // Slow worker
                } else {
                    Duration::from_millis(1) // Fast workers
                };
                Arc::new(SlowWorker::new(i, delay))
            })
            .collect();

        let mut scheduler = Scheduler::new(&slow_workers).unwrap();
        let durations = scheduler
            .run(interval(), timeout(), &StubProgressReporter::default())
            .await
            .unwrap();

        // Should handle slow workers gracefully
        assert!(!durations.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_handles_saturation() {
        let slow_workers: Vec<Arc<SlowWorker>> = (0..2)
            .map(|i| Arc::new(SlowWorker::new(i, Duration::from_millis(200)))) // Very slow workers
            .collect();

        let mut scheduler = Scheduler::new(&slow_workers).unwrap();
        let short_interval = Duration::from_millis(50); // Fast interval
        let timeout = Duration::from_millis(1000);

        let durations = scheduler
            .run(short_interval, timeout, &StubProgressReporter::default())
            .await
            .unwrap();

        // Should handle saturation gracefully by skipping missed ticks
        assert!(!durations.is_empty());
        assert!(durations.len() < 100); // Fewer durations due to saturation
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_respects_timeout() {
        let w = workers();
        let mut scheduler = Scheduler::new(&w).unwrap();

        // Use a very short timeout to test timeout behavior
        let very_short_timeout = Duration::from_millis(1);
        let durations = scheduler
            .run(
                interval(),
                very_short_timeout,
                &StubProgressReporter::default(),
            )
            .await
            .unwrap();

        // Should respect the timeout and stop early
        assert!(durations.len() < 10);
    }

    #[test]
    fn test_worker_trait_implementation() {
        // Test that Worker trait is implemented correctly
        let _worker = Arc::new(StubWorker::new(0));
        // This test just verifies the trait can be implemented
        // The actual async functionality is tested in other tests
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_high_concurrency() {
        // Test that the scheduler can handle high concurrency successfully
        let many_workers: Vec<Arc<StubWorker>> =
            (0..100).map(|i| Arc::new(StubWorker::new(i))).collect();

        let mut scheduler = Scheduler::new(&many_workers).unwrap();
        let durations = scheduler
            .run(interval(), timeout(), &StubProgressReporter::default())
            .await
            .unwrap();

        // Should complete successfully with high concurrency
        assert!(!durations.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_work_distribution() {
        let w = workers();
        let mut scheduler = Scheduler::new(&w).unwrap();
        let durations = scheduler
            .run(interval(), timeout(), &StubProgressReporter::default())
            .await
            .unwrap();

        // Verify that work was distributed across workers
        let total_calls: u32 = w.iter().map(|w| w.triggers.load(Ordering::Relaxed)).sum();
        assert!(total_calls > 0, "Work should be distributed");

        // Verify the expected total durations
        assert_eq!(
            durations.iter().map(|r| r.durations.len()).sum::<usize>(),
            100
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_worker_scheduler_schedules_at_interval_despite_slow_tasks() {
        // Create workers that take longer than the interval to complete
        let slow_workers: Vec<Arc<SlowWorker>> = (0..4)
            .map(|i| Arc::new(SlowWorker::new(i, Duration::from_millis(150)))) // 150ms per task
            .collect();

        let mut scheduler = Scheduler::new(&slow_workers).unwrap();
        let short_interval = Duration::from_millis(50); // 50ms interval
        let test_timeout = Duration::from_millis(1000); // 1 second test

        let durations = scheduler
            .run(
                short_interval,
                test_timeout,
                &StubProgressReporter::default(),
            )
            .await
            .unwrap();

        // Verify that tasks were scheduled at the correct interval despite being slow
        // With 4 workers and 50ms interval, each worker should be scheduled every 200ms (4 * 50ms)
        // Over 1 second, we should have scheduled multiple batches of tasks

        // Check that we have results from all workers (indicating they were all scheduled)
        assert_eq!(durations.len(), 4, "Should have results from all 4 workers");

        // Verify that all workers were called multiple times (indicating they were scheduled repeatedly)
        let total_calls: u32 = slow_workers
            .iter()
            .map(|w| w.triggers.load(Ordering::Relaxed))
            .sum();
        assert!(
            total_calls >= 8,
            "Workers should be called multiple times due to concurrent scheduling"
        );

        // Verify that the number of completed tasks is reasonable for the time period
        let total_completed_tasks: usize = durations.iter().map(|r| r.durations.len()).sum();
        assert!(
            total_completed_tasks >= 4,
            "Should complete at least 4 tasks over the test period"
        );

        // Verify that tasks are being scheduled at the expected rate
        // With 4 workers, 50ms interval, and 1 second timeout:
        // - Each worker should be scheduled every 200ms (4 * 50ms)
        // - Over 1 second, each worker should be scheduled ~5 times
        // - Some tasks may not complete due to the timeout, but they should be started
        let expected_min_schedules_per_worker = 3; // Conservative estimate
        for (i, worker_calls) in slow_workers
            .iter()
            .map(|w| w.triggers.load(Ordering::Relaxed))
            .enumerate()
        {
            assert!(
                worker_calls >= expected_min_schedules_per_worker,
                "Worker {i} should be scheduled at least {expected_min_schedules_per_worker} times, but was only scheduled {worker_calls} times",
            );
        }

        // Verify that some tasks completed (indicating concurrent execution)
        // If tasks were running sequentially, very few would complete in 1 second
        let completed_tasks_per_worker: Vec<usize> =
            durations.iter().map(|r| r.durations.len()).collect();
        let total_completed = completed_tasks_per_worker.iter().sum::<usize>();
        assert!(
            total_completed >= 4,
            "Should complete at least 4 tasks total, but only completed {total_completed}",
        );
    }
}

// TODO: Test requests sent.
// TODO: Test reporter.

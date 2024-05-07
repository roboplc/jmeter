use std::io::BufRead;
use std::time::Duration;

use roboplc::metrics::{counter, gauge, histogram};
use roboplc::prelude::*;
use roboplc::time::interval;

fn num_cpus() -> usize {
    let f = std::fs::File::open("/proc/cpuinfo").unwrap();
    let reader = std::io::BufReader::new(f);
    let lines = reader.lines();
    let mut count = 0;
    for line in lines {
        let line = line.unwrap();
        if line.starts_with("processor") {
            count += 1;
        }
    }
    count
}

struct Meter {
    name: &'static str,
    cpu_ids: [usize; 1],
    interval: Duration,
}

impl WorkerOptions for Meter {
    fn worker_name(&self) -> &'static str {
        self.name
    }

    fn worker_scheduling(&self) -> Scheduling {
        Scheduling::FIFO
    }

    fn worker_priority(&self) -> Option<i32> {
        Some(90)
    }

    fn worker_cpu_ids(&self) -> Option<&[usize]> {
        Some(&self.cpu_ids)
    }

    fn worker_is_blocking(&self) -> bool {
        true
    }
}

impl Worker<(), ()> for Meter {
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn run(&mut self, _context: &Context<(), ()>) -> WResult {
        let mut prev: Option<Monotonic> = None;
        let mut max_jitter = Duration::from_secs(0);
        // executed once, leak is safe
        let j_gauge = gauge!(format!("CPU{:02}::jitter::abs_max", self.cpu_ids[0]));
        let j_histrogram = histogram!(format!("CPU{:02}::jitter", self.cpu_ids[0]));
        for _ in interval(self.interval) {
            if self.cpu_ids[0] == 0 {
                counter!("ITERATIONS").increment(1);
            }
            let now = Monotonic::now();
            if let Some(p) = prev {
                let jitter = now.duration_since(p).diff_abs(self.interval).as_micros();
                if jitter > max_jitter.as_micros() {
                    max_jitter = Duration::from_micros(jitter as u64);
                    j_gauge.set(max_jitter.as_micros() as f64);
                }
                j_histrogram.record(jitter as f64);
            }
            prev = Some(now);
        }
        Ok(())
    }
}

#[allow(clippy::cast_precision_loss)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let maybe_interval_str = option_env!("INTERVAL");
    let interval_us: u64 = if let Some(interval_str) = maybe_interval_str {
        interval_str.parse().expect("INTERVAL must be a number")
    } else {
        1_000
    };
    let interval = Duration::from_micros(interval_us);
    roboplc::configure_logger(roboplc::LevelFilter::Info);
    roboplc::metrics_exporter()
        .set_bucket_duration(Duration::from_secs(600))?
        .install()?;
    let _kernel_config = roboplc::thread_rt::SystemConfig::new()
        .set("kernel/sched_rt_runtime_us", -1)
        .apply()?;
    roboplc::thread_rt::prealloc_heap(10_000_000)?;
    let mut controller = Controller::<(), ()>::new();
    let n_cpus = num_cpus();
    gauge!("CPUS_TOTAL").set(n_cpus as f64);
    gauge!("INTERVAL_US").set(interval.as_micros() as f64);
    for cpu in 0..n_cpus {
        // executed once, leak is safe
        let name = Box::leak(format!("jmCPU{:02}", cpu).into_boxed_str());
        controller.spawn_worker(Meter {
            name,
            cpu_ids: [cpu],
            interval,
        })?;
    }
    controller.register_signals(Duration::from_secs(5))?;
    controller.block();
    Ok(())
}

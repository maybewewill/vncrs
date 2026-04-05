use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct FpsCounter {
    times: VecDeque<Instant>,
    sizes: VecDeque<usize>,
    dirty: VecDeque<usize>,
    total: u64,
    start: Instant,
    last_print: Instant,
}

impl FpsCounter {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            times: VecDeque::with_capacity(300),
            sizes: VecDeque::with_capacity(300),
            dirty: VecDeque::with_capacity(300),
            total: 0,
            start: now,
            last_print: now,
        }
    }

    pub fn frame(&mut self, bytes: usize, dirty_count: usize) {
        let now = Instant::now();
        self.times.push_back(now);
        self.sizes.push_back(bytes);
        self.dirty.push_back(dirty_count);
        self.total += 1;

        let window = Duration::from_secs(2);
        while self
            .times
            .front()
            .is_some_and(|t| now.duration_since(*t) > window)
        {
            self.times.pop_front();
            self.sizes.pop_front();
            self.dirty.pop_front();
        }

        if now.duration_since(self.last_print) >= Duration::from_secs(1) {
            self.print_stats();
            self.last_print = now;
        }
    }

    pub fn fps(&self) -> f64 {
        if self.times.len() < 2 {
            return 0.0;
        }
        let elapsed = self
            .times
            .back()
            .unwrap()
            .duration_since(*self.times.front().unwrap())
            .as_secs_f64();
        if elapsed > 0.0 {
            (self.times.len() - 1) as f64 / elapsed
        } else {
            0.0
        }
    }

    pub fn print_stats(&self) {
        let fps = self.fps();
        let avg_size = if self.sizes.is_empty() {
            0
        } else {
            self.sizes.iter().sum::<usize>() / self.sizes.len()
        };
        let avg_dirty = if self.dirty.is_empty() {
            0.0
        } else {
            self.dirty.iter().sum::<usize>() as f64 / self.dirty.len() as f64
        };
        let mbps = if self.times.len() < 2 {
            0.0
        } else {
            let elapsed = self
                .times
                .back()
                .unwrap()
                .duration_since(*self.times.front().unwrap())
                .as_secs_f64();
            if elapsed > 0.0 {
                self.sizes.iter().sum::<usize>() as f64 * 8.0 / elapsed / 1_000_000.0
            } else {
                0.0
            }
        };

        let icon = if fps >= 25.0 {
            ""
        } else if fps >= 15.0 {
            ""
        } else {
            ""
        };
        let size = if avg_size >= 1_048_576 {
            format!("{:.1}MB", avg_size as f64 / 1_048_576.0)
        } else if avg_size >= 1024 {
            format!("{:.0}KB", avg_size as f64 / 1024.0)
        } else {
            format!("{}B", avg_size)
        };

        println!(
            "{} {:5.1}fps {:>7} {:5.1}Mbps d:{:.0} #{} {}s",
            icon,
            fps,
            size,
            mbps,
            avg_dirty,
            self.total,
            self.start.elapsed().as_secs()
        );
    }
}

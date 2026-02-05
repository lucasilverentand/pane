use std::time::Duration;
use sysinfo::System;
use tokio::sync::mpsc;

use crate::event::AppEvent;

#[derive(Clone, Debug, Default)]
pub struct SystemStats {
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub load_avg_1: f64,
    pub disk_usage_percent: f32,
}

impl SystemStats {
    pub fn format_cpu(&self) -> String {
        format!("CPU {}%", self.cpu_percent as u32)
    }

    pub fn format_memory(&self) -> String {
        format!("MEM {}%", self.memory_percent as u32)
    }

    pub fn format_load(&self) -> String {
        format!("LOAD {:.2}", self.load_avg_1)
    }

    pub fn format_disk(&self) -> String {
        format!("DISK {}%", self.disk_usage_percent as u32)
    }
}

pub fn start_stats_collector(
    event_tx: mpsc::UnboundedSender<AppEvent>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        let mut sys = System::new();
        let interval = Duration::from_secs(interval_secs.max(1));

        loop {
            sys.refresh_cpu_usage();
            sys.refresh_memory();

            // Need a short sleep after refresh_cpu_usage for accurate readings
            tokio::time::sleep(Duration::from_millis(200)).await;
            sys.refresh_cpu_usage();

            let cpu_percent = sys.global_cpu_usage();
            let memory_percent = if sys.total_memory() > 0 {
                (sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0) as f32
            } else {
                0.0
            };

            let load_avg = System::load_average();

            let disk_usage_percent = {
                let disks = sysinfo::Disks::new_with_refreshed_list();
                let mut total_space = 0u64;
                let mut available_space = 0u64;
                for disk in disks.list() {
                    total_space += disk.total_space();
                    available_space += disk.available_space();
                }
                if total_space > 0 {
                    ((total_space - available_space) as f64 / total_space as f64 * 100.0) as f32
                } else {
                    0.0
                }
            };

            let stats = SystemStats {
                cpu_percent,
                memory_percent,
                load_avg_1: load_avg.one,
                disk_usage_percent,
            };

            if event_tx.send(AppEvent::SystemStats(stats)).is_err() {
                break;
            }

            tokio::time::sleep(interval.saturating_sub(Duration::from_millis(200))).await;
        }
    });
}

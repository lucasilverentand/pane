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

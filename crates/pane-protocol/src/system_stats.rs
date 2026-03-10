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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_all_zeros() {
        let stats = SystemStats::default();
        assert!((stats.cpu_percent - 0.0).abs() < f32::EPSILON);
        assert!((stats.memory_percent - 0.0).abs() < f32::EPSILON);
        assert!((stats.load_avg_1 - 0.0).abs() < f64::EPSILON);
        assert!((stats.disk_usage_percent - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn format_cpu_truncates_to_integer() {
        let stats = SystemStats {
            cpu_percent: 42.7,
            ..Default::default()
        };
        assert_eq!(stats.format_cpu(), "CPU 42%");
    }

    #[test]
    fn format_cpu_zero() {
        let stats = SystemStats::default();
        assert_eq!(stats.format_cpu(), "CPU 0%");
    }

    #[test]
    fn format_cpu_hundred() {
        let stats = SystemStats {
            cpu_percent: 100.0,
            ..Default::default()
        };
        assert_eq!(stats.format_cpu(), "CPU 100%");
    }

    #[test]
    fn format_memory_truncates() {
        let stats = SystemStats {
            memory_percent: 67.9,
            ..Default::default()
        };
        assert_eq!(stats.format_memory(), "MEM 67%");
    }

    #[test]
    fn format_memory_zero() {
        let stats = SystemStats::default();
        assert_eq!(stats.format_memory(), "MEM 0%");
    }

    #[test]
    fn format_load_two_decimals() {
        let stats = SystemStats {
            load_avg_1: 1.5,
            ..Default::default()
        };
        assert_eq!(stats.format_load(), "LOAD 1.50");
    }

    #[test]
    fn format_load_small_value() {
        let stats = SystemStats {
            load_avg_1: 0.01,
            ..Default::default()
        };
        assert_eq!(stats.format_load(), "LOAD 0.01");
    }

    #[test]
    fn format_load_large_value() {
        let stats = SystemStats {
            load_avg_1: 12.345,
            ..Default::default()
        };
        assert_eq!(stats.format_load(), "LOAD 12.35");
    }

    #[test]
    fn format_disk_truncates() {
        let stats = SystemStats {
            disk_usage_percent: 55.9,
            ..Default::default()
        };
        assert_eq!(stats.format_disk(), "DISK 55%");
    }

    #[test]
    fn format_disk_zero() {
        let stats = SystemStats::default();
        assert_eq!(stats.format_disk(), "DISK 0%");
    }

    #[test]
    fn clone_preserves_values() {
        let stats = SystemStats {
            cpu_percent: 33.3,
            memory_percent: 66.6,
            load_avg_1: 4.56,
            disk_usage_percent: 80.0,
        };
        let cloned = stats.clone();
        assert!((cloned.cpu_percent - 33.3).abs() < f32::EPSILON);
        assert!((cloned.memory_percent - 66.6).abs() < f32::EPSILON);
        assert!((cloned.load_avg_1 - 4.56).abs() < f64::EPSILON);
        assert!((cloned.disk_usage_percent - 80.0).abs() < f32::EPSILON);
    }

    #[test]
    fn debug_format() {
        let stats = SystemStats {
            cpu_percent: 10.0,
            memory_percent: 20.0,
            load_avg_1: 0.5,
            disk_usage_percent: 30.0,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("cpu_percent"));
        assert!(debug.contains("memory_percent"));
        assert!(debug.contains("load_avg_1"));
        assert!(debug.contains("disk_usage_percent"));
    }

    #[test]
    fn all_fields_set_independently() {
        let stats = SystemStats {
            cpu_percent: 99.9,
            memory_percent: 0.1,
            load_avg_1: 0.0,
            disk_usage_percent: 100.0,
        };
        assert_eq!(stats.format_cpu(), "CPU 99%");
        assert_eq!(stats.format_memory(), "MEM 0%");
        assert_eq!(stats.format_load(), "LOAD 0.00");
        assert_eq!(stats.format_disk(), "DISK 100%");
    }
}

//! Host metrics from `/proc` and `statvfs`. The parsers are pure so the unit
//! tests run on fixtures; [`Collector`] adds the two reads that need the live
//! system (the CPU delta between scrapes and `statvfs`).

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How many `/proc/stat` modes are reported (`user`..`steal`).
const CPU_MODES: usize = 8;
const CPU_NAMES: [&str; CPU_MODES] = [
    "user", "nice", "system", "idle", "iowait", "irq", "softirq", "steal",
];

/// Mode labels of `pulsar_cpu_seconds_total`, in [`CpuTimes::seconds`] order.
#[must_use]
pub fn mode_names() -> &'static [&'static str] {
    &CPU_NAMES
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTimes {
    /// CPU-seconds per mode, in [`CPU_NAMES`] order.
    pub seconds: [f64; CPU_MODES],
}

impl CpuTimes {
    fn total(self) -> f64 {
        self.seconds.iter().sum()
    }

    fn idle(self) -> f64 {
        self.seconds[3] + self.seconds[4]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Temperature {
    pub zone: String,
    pub kind: String,
    pub celsius: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Filesystem {
    pub mount: &'static str,
    pub total_bytes: f64,
    pub free_bytes: f64,
}

#[derive(Clone, Debug, Default)]
pub struct SystemSample {
    pub cpu: CpuTimes,
    /// 0..1 over the interval since the previous scrape (since boot on the first).
    pub cpu_usage: f64,
    pub loads: [f64; 3],
    pub temperatures: Vec<Temperature>,
    pub memory_total: f64,
    pub memory_available: f64,
    pub filesystems: Vec<Filesystem>,
    pub uptime: f64,
}

/// Mounts whose capacity is exported.
const MOUNTS: [&str; 2] = ["/data", "/"];

/// Live collector: keeps the previous CPU sample for the usage ratio.
#[derive(Debug)]
pub struct Collector {
    root: PathBuf,
    previous: Mutex<Option<CpuTimes>>,
}

impl Collector {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            previous: Mutex::new(None),
        }
    }

    /// Filesystems and temperatures come from the live system even when `root`
    /// points at a fixture tree for tests.
    #[must_use]
    pub fn sample(&self) -> SystemSample {
        let cpu = read_proc_stat(&self.root).unwrap_or_default();
        let previous = self
            .previous
            .lock()
            .map_or(None, |mut prev| prev.replace(cpu));
        SystemSample {
            cpu,
            cpu_usage: cpu_usage(previous, cpu),
            loads: read_loadavg(&self.root).unwrap_or_default(),
            temperatures: read_temperatures(&self.root),
            memory_total: read_meminfo(&self.root).map_or(0.0, |(total, _)| total),
            memory_available: read_meminfo(&self.root).map_or(0.0, |(_, available)| available),
            filesystems: read_filesystems(),
            uptime: read_uptime(&self.root).unwrap_or(0.0),
        }
    }
}

fn cpu_usage(previous: Option<CpuTimes>, current: CpuTimes) -> f64 {
    let (total, idle) = previous.map_or((current.total(), current.idle()), |prev| {
        (current.total() - prev.total(), current.idle() - prev.idle())
    });
    if total <= 0.0 {
        return 0.0;
    }
    (1.0 - (idle / total)).clamp(0.0, 1.0)
}

fn read_text(root: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(root.join(relative)).ok()
}

/// `/proc/stat` first `cpu` line. Unknown columns are ignored, so newer kernels
/// do not break the parse.
#[must_use]
pub fn parse_proc_stat(text: &str, hz: f64) -> Option<CpuTimes> {
    let line = text
        .lines()
        .find(|line| line.split_whitespace().next() == Some("cpu"))?;
    let mut fields = line.split_whitespace().skip(1);
    let mut seconds = [0.0; CPU_MODES];
    for slot in &mut seconds {
        *slot = fields.next()?.parse::<f64>().ok()? / hz;
    }
    Some(CpuTimes { seconds })
}

fn read_proc_stat(root: &Path) -> Option<CpuTimes> {
    let hz = clock_ticks();
    parse_proc_stat(&read_text(root, "proc/stat")?, hz)
}

fn clock_ticks() -> f64 {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `sysconf` only reads the process's cached system value.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if hz > 0 { hz as f64 } else { 100.0 }
    }
    #[cfg(not(target_os = "linux"))]
    {
        100.0
    }
}

#[must_use]
pub fn parse_loadavg(text: &str) -> Option<[f64; 3]> {
    let mut fields = text.split_whitespace();
    let mut loads = [0.0; 3];
    for slot in &mut loads {
        *slot = fields.next()?.parse().ok()?;
    }
    Some(loads)
}

fn read_loadavg(root: &Path) -> Option<[f64; 3]> {
    parse_loadavg(&read_text(root, "proc/loadavg")?)
}

#[must_use]
pub fn parse_meminfo(text: &str) -> Option<(f64, f64)> {
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let key = fields.next()?;
        let kib = fields.next()?.parse::<f64>().ok()?;
        match key {
            "MemTotal:" => total = Some(kib * 1024.0),
            "MemAvailable:" => available = Some(kib * 1024.0),
            _ => {}
        }
    }
    Some((total?, available?))
}

fn read_meminfo(root: &Path) -> Option<(f64, f64)> {
    parse_meminfo(&read_text(root, "proc/meminfo")?)
}

#[must_use]
pub fn parse_uptime(text: &str) -> Option<f64> {
    text.split_whitespace().next()?.parse().ok()
}

fn read_uptime(root: &Path) -> Option<f64> {
    parse_uptime(&read_text(root, "proc/uptime")?)
}

fn read_temperatures(root: &Path) -> Vec<Temperature> {
    let dir = root.join("sys/class/thermal");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut zones: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .map(|e| e.path())
        .collect();
    zones.sort();
    zones
        .into_iter()
        .filter_map(|path| {
            let zone = path.file_name()?.to_string_lossy().into_owned();
            let milli: f64 = read_text(&path, "temp")?.trim().parse().ok()?;
            let kind = read_text(&path, "type")
                .map(|s| s.trim().to_owned())
                .unwrap_or_default();
            Some(Temperature {
                zone,
                kind,
                celsius: milli / 1000.0,
            })
        })
        .collect()
}

fn read_filesystems() -> Vec<Filesystem> {
    MOUNTS
        .iter()
        .filter_map(|mount| {
            let (total_bytes, free_bytes) = statvfs(mount)?;
            Some(Filesystem {
                mount,
                total_bytes,
                free_bytes,
            })
        })
        .collect()
}

/// Total and free bytes of a mount (free as `f_bavail`, like `df` for a user).
fn statvfs(mount: &str) -> Option<(f64, f64)> {
    let path = CString::new(mount).ok()?;
    // SAFETY: `path` is a valid NUL-terminated string and `stat` points at
    // writable memory of the size `statvfs` expects.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are valid for the duration of the call.
    let rc = unsafe { libc::statvfs(path.as_ptr(), &raw mut stat) };
    if rc != 0 {
        return None;
    }
    let block = if stat.f_frsize != 0 {
        stat.f_frsize
    } else {
        stat.f_bsize
    } as f64;
    Some((block * stat.f_blocks as f64, block * stat.f_bavail as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_STAT: &str = "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 50 0 50 350 50 0 0 0 0 0\n";
    const PROC_STAT_LATER: &str = "cpu  130 0 130 860 0 0 0 0 0 0\nintr 1\n";
    const MEMINFO: &str =
        "MemTotal:        2000000 kB\nMemFree:          100000 kB\nMemAvailable:     500000 kB\n";
    const LOADAVG: &str = "0.31 0.52 0.75 1/345 6789\n";
    const UPTIME: &str = "12345.67 100.00\n";

    #[test]
    fn proc_stat_is_parsed_into_seconds_per_mode() {
        let times = parse_proc_stat(PROC_STAT, 100.0).unwrap();
        assert_eq!(times.seconds[0], 1.0, "user");
        assert_eq!(times.seconds[2], 1.0, "system");
        assert_eq!(times.seconds[3], 7.0, "idle");
        assert_eq!(times.seconds[4], 1.0, "iowait");
        assert_eq!(times.seconds[5..], [0.0; 3]);
    }

    #[test]
    fn cpu_usage_is_the_delta_not_the_boot_average() {
        let first = parse_proc_stat(PROC_STAT, 100.0).unwrap();
        let later = parse_proc_stat(PROC_STAT_LATER, 100.0).unwrap();
        // 60 new jiffies: 30 busy, 30 idle.
        assert!((cpu_usage(Some(first), later) - 0.5).abs() < 1e-9);
        assert!((cpu_usage(None, first) - 0.2).abs() < 1e-9);
        assert_eq!(cpu_usage(Some(first), first), 0.0);
    }

    #[test]
    fn meminfo_reports_total_and_available_in_bytes() {
        assert_eq!(
            parse_meminfo(MEMINFO),
            Some((2_000_000.0 * 1024.0, 500_000.0 * 1024.0))
        );
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None);
    }

    #[test]
    fn loadavg_and_uptime_parse_their_first_fields() {
        assert_eq!(parse_loadavg(LOADAVG), Some([0.31, 0.52, 0.75]));
        assert_eq!(parse_uptime(UPTIME), Some(12345.67));
        assert_eq!(parse_loadavg("broken"), None);
    }

    #[test]
    fn malformed_input_is_none_not_a_panic() {
        assert_eq!(parse_proc_stat("intr 0\n", 100.0), None);
        assert_eq!(parse_proc_stat("cpu 1 2\n", 100.0), None);
        assert_eq!(parse_proc_stat("cpu x y z\n", 100.0), None);
    }
}

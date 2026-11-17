use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MAX_SOURCE: usize = 64 * 1024;
const MAX_OUTPUT: u64 = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(15);
const MAX_RESIDENT: u64 = 512 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(0);

struct Job {
    directory: PathBuf,
    child: Option<Child>,
}

impl Drop for Job {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub fn render(text: &str, dark: bool, cancelled: &AtomicBool) -> Result<Vec<u8>, String> {
    if text.len() > MAX_SOURCE {
        return Err("Mermaid source exceeds 64 KiB".into());
    }
    let directory = std::env::temp_dir().join(format!(
        "gezi-mermaid-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|err| err.to_string())?;
    let mut job = Job {
        directory,
        child: None,
    };
    let input = job.directory.join("input.json");
    let output = job.directory.join("output.svg");
    fs::write(
        &input,
        serde_json::to_vec(&(text, dark)).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    let mut command = Command::new(std::env::current_exe().map_err(|err| err.to_string())?);
    command
        .arg("--internal-mermaid")
        .env_clear()
        .env("XDG_CACHE_HOME", &job.directory)
        .stdin(File::open(input).map_err(|err| err.to_string())?)
        .stdout(File::create(&output).map_err(|err| err.to_string())?)
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            let limit = nix::libc::rlimit {
                rlim_cur: MAX_OUTPUT,
                rlim_max: MAX_OUTPUT,
            };
            if nix::libc::setrlimit(nix::libc::RLIMIT_FSIZE, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("Mermaid cancelled".into());
    }
    job.child = Some(command.spawn().map_err(|err| err.to_string())?);
    wait(&mut job, cancelled, DEADLINE)?;
    let mut bytes = Vec::new();
    File::open(output)
        .map_err(|err| err.to_string())?
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_OUTPUT {
        return Err("invalid Mermaid output size".into());
    }
    Ok(bytes)
}

fn wait(job: &mut Job, cancelled: &AtomicBool, deadline: Duration) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) || started.elapsed() >= deadline {
            return Err("Mermaid cancelled or timed out".into());
        }
        if resident_bytes(job.child.as_ref().unwrap().id())
            .is_some_and(|bytes| bytes > MAX_RESIDENT)
        {
            return Err("Mermaid exceeded memory budget".into());
        }
        if let Some(status) = job
            .child
            .as_mut()
            .unwrap()
            .try_wait()
            .map_err(|err| err.to_string())?
        {
            if !status.success() {
                return Err("Mermaid preview unavailable".into());
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<nix::libc::proc_taskinfo>::uninit();
    let size = std::mem::size_of::<nix::libc::proc_taskinfo>() as i32;
    let read = unsafe {
        nix::libc::proc_pidinfo(
            pid as i32,
            nix::libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    (read == size).then(|| unsafe { info.assume_init().pti_resident_size })
}

#[cfg(not(target_os = "macos"))]
fn resident_bytes(_pid: u32) -> Option<u64> {
    None
}

pub fn worker() -> ExitCode {
    match work() {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn work() -> Result<(), String> {
    let mut input = Vec::new();
    std::io::stdin()
        .take((MAX_SOURCE * 8 + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|err| err.to_string())?;
    if input.len() > MAX_SOURCE * 8 {
        return Err("input too large".into());
    }
    let (text, dark): (String, bool) =
        serde_json::from_slice(&input).map_err(|err| err.to_string())?;
    let svg = generate(&text, dark)?;
    std::io::stdout()
        .write_all(svg.as_bytes())
        .map_err(|err| err.to_string())
}

fn generate(text: &str, dark: bool) -> Result<String, String> {
    use mermaid_rs_renderer::{
        DiagramKind, LayoutConfig, Theme, compute_layout, parse_mermaid_strict, render_svg,
    };
    if text.len() > MAX_SOURCE {
        return Err("source too large".into());
    }
    let parsed = parse_mermaid_strict(text).map_err(|err| err.to_string())?;
    let graph = &parsed.graph;
    if !matches!(
        graph.kind,
        DiagramKind::Flowchart
            | DiagramKind::Sequence
            | DiagramKind::State
            | DiagramKind::Class
            | DiagramKind::Er
            | DiagramKind::Mindmap
    ) {
        return Err("unsupported diagram type".into());
    }
    let nodes = graph.nodes.len() + graph.mindmap.nodes.len();
    let annotations = graph.subgraphs.len()
        + graph.sequence_frames.len()
        + graph.sequence_notes.len()
        + graph.sequence_activations.len()
        + graph.sequence_boxes.len()
        + graph.state_notes.len();
    if nodes > 200 || graph.edges.len() > 400 || annotations > 200 {
        return Err("diagram too complex".into());
    }
    let mut theme = if dark {
        Theme::dark()
    } else {
        Theme::mermaid_default()
    };
    theme.font_family = "Arial, Heiti SC, Songti SC, sans-serif".into();
    theme.font_size = 24.0;
    let config = LayoutConfig::default();
    let layout = compute_layout(graph, &theme, &config);
    let svg = render_svg(&layout, &theme, &config);
    if svg.len() as u64 > MAX_OUTPUT {
        return Err("output too large".into());
    }
    Ok(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn resident_memory_is_available_on_supported_platform() {
        assert!(resident_bytes(std::process::id()).unwrap() > 0);
    }

    #[test]
    fn deadline_and_cancellation_reap_worker_and_remove_session_files() {
        for cancelled in [false, true] {
            let directory = std::env::temp_dir().join(format!(
                "gezi-mermaid-test-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("cache"), b"font cache").unwrap();
            let child = Command::new("/bin/sleep").arg("5").spawn().unwrap();
            let pid = child.id();
            let mut job = Job {
                directory: directory.clone(),
                child: Some(child),
            };
            let started = Instant::now();
            assert!(
                wait(
                    &mut job,
                    &AtomicBool::new(cancelled),
                    Duration::from_millis(20)
                )
                .is_err()
            );
            drop(job);
            assert!(started.elapsed() < Duration::from_secs(1));
            assert!(!directory.exists());
            assert_eq!(
                unsafe { nix::libc::waitpid(pid as i32, std::ptr::null_mut(), nix::libc::WNOHANG) },
                -1
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(nix::libc::ECHILD)
            );
        }
    }

    #[test]
    fn supported_diagrams_produce_real_images() {
        for source in [
            "flowchart LR\nA[开始] --> B[完成]",
            "sequenceDiagram\nAlice->>Bob: hello",
            "stateDiagram-v2\n[*] --> Ready",
            "classDiagram\nAnimal <|-- Duck",
            "erDiagram\nCUSTOMER ||--o{ ORDER : places",
            "mindmap\n  root((Root))\n    Child",
        ] {
            for dark in [false, true] {
                let svg = generate(source, dark).unwrap();
                let raster = crate::image_loader::decode(svg.as_bytes()).unwrap();
                assert!(raster.width > 10 && raster.height > 10);
                assert!(raster.pixels.pixels().any(|pixel| pixel[3] > 0));
            }
        }
    }

    #[test]
    fn rejects_unsupported_invalid_and_oversized_sources() {
        for source in [
            "pie\n\"A\": 1".to_owned(),
            "not a diagram".into(),
            "flowchart LR\nsubgraph A\nB --> C".into(),
            "x".repeat(MAX_SOURCE + 1),
        ] {
            assert!(generate(&source, false).is_err());
        }
        let source = format!(
            "mindmap\n  root\n{}",
            (0..201)
                .map(|index| format!("    Node{index}\n"))
                .collect::<String>()
        );
        assert!(generate(&source, false).is_err());
    }
}

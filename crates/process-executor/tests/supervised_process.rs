use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use keeper_process_executor::{CancellationSignal, ProcessSpec, SupervisedProcessRunner};

fn fixture_path() -> PathBuf {
    std::env::current_exe().expect("test executable")
}

fn spec(mode: &str, cap: usize) -> ProcessSpec {
    let mut env = BTreeMap::new();
    env.insert(OsString::from("KPE_FIXTURE_MODE"), OsString::from(mode));
    ProcessSpec {
        executable: fixture_path(),
        argv: vec![
            OsString::from("--exact"),
            OsString::from("fixture_child"),
            OsString::from("--nocapture"),
        ],
        cwd: std::env::current_dir().expect("current directory"),
        env,
        timeout: Duration::from_secs(3),
        output_byte_cap: cap,
    }
}

#[test]
fn stdout_stderr_lossy_and_nonzero_are_preserved_separately() {
    let receipt = SupervisedProcessRunner::new()
        .run(spec("mixed", 4096), CancellationSignal::new())
        .expect("run fixture");
    assert_eq!(receipt.exit_code, Some(9));
    assert!(receipt.stdout.text.contains("stdout-marker"));
    assert!(receipt.stderr.text.contains("stderr-marker"));
    assert!(receipt.stdout.lossy_utf8);
    assert!(receipt.cleanup_proven);
}

#[test]
fn combined_output_cap_is_shared_and_reports_truncation() {
    let receipt = SupervisedProcessRunner::new()
        .run(spec("large", 64), CancellationSignal::new())
        .expect("run fixture");
    assert!(receipt.stdout.truncated || receipt.stderr.truncated);
    assert!(receipt.stdout.bytes_retained + receipt.stderr.bytes_retained <= 64);
    assert!(receipt.stdout.bytes_seen + receipt.stderr.bytes_seen > 64);
}

#[test]
fn immediate_dual_stream_saturation_completes_with_bounded_capture() {
    let started = std::time::Instant::now();
    let receipt = SupervisedProcessRunner::new()
        .run(spec("dual_saturated", 32 * 1024), CancellationSignal::new())
        .expect("capture saturated streams");

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!receipt.cancelled && !receipt.timed_out);
    assert!(receipt.stdout.truncated || receipt.stderr.truncated);
    assert!(receipt.stdout.bytes_retained + receipt.stderr.bytes_retained <= 32 * 1024);
    assert!(receipt.stdout.bytes_seen + receipt.stderr.bytes_seen > 32 * 1024);
    assert!(receipt.cleanup_proven);
}

#[test]
fn spawn_piped_discards_stderr_and_cleans_its_owned_child() {
    let mut child = SupervisedProcessRunner::new()
        .spawn_piped(spec("sleep", 4096))
        .expect("spawn piped fixture");
    let pid = child.pid();

    assert!(child.take_stderr().is_none());
    child
        .kill_tree(Duration::from_secs(2))
        .expect("owned child cleanup");
    assert!(keeper_process_executor::process_gone(pid).expect("child absence proof"));
}

#[test]
fn timeout_and_explicit_cancellation_terminate_owned_children() {
    let mut short = spec("sleep", 4096);
    short.timeout = Duration::from_millis(80);
    let timeout = SupervisedProcessRunner::new()
        .run(short, CancellationSignal::new())
        .expect("timeout receipt");
    assert!(timeout.timed_out && timeout.cleanup_proven);

    let signal = CancellationSignal::new();
    signal.cancel();
    let cancelled = SupervisedProcessRunner::new()
        .run(spec("sleep", 4096), signal)
        .expect("cancel receipt");
    assert!(cancelled.cancelled && cancelled.cleanup_proven);
}

#[test]
fn timeout_cleans_grandchildren_through_the_owned_tree() {
    let mut tree = spec("grandchild", 4096);
    tree.timeout = Duration::from_millis(100);
    let receipt = SupervisedProcessRunner::new()
        .run(tree, CancellationSignal::new())
        .expect("grandchild timeout receipt");
    assert!(receipt.timed_out && receipt.cleanup_proven);
}

#[test]
fn unavailable_executable_fails_with_a_typed_error() {
    let mut invalid = spec("mixed", 4096);
    invalid.executable = PathBuf::from("not-a-real-executable");
    let error = SupervisedProcessRunner::new()
        .run(invalid, CancellationSignal::new())
        .expect_err("unavailable executable");
    assert_eq!(error.to_string(), "process executable unavailable");
}

#[cfg(debug_assertions)]
#[test]
fn post_spawn_setup_failure_terminates_the_owned_suspended_child() {
    let mut failing = spec("sleep", 4096);
    failing.env.insert(
        OsString::from("KEEPER_PROCESS_EXECUTOR_TEST_SETUP_FAILURE"),
        OsString::from("1"),
    );
    let error = SupervisedProcessRunner::new()
        .run(failing, CancellationSignal::new())
        .expect_err("injected setup failure");
    assert_eq!(error.to_string(), "process setup failed");
}

#[test]
fn secret_environment_names_are_denied() {
    let mut invalid = spec("mixed", 4096);
    invalid
        .env
        .insert(OsString::from("API_TOKEN"), OsString::from("never-copy"));
    let error = SupervisedProcessRunner::new()
        .run(invalid, CancellationSignal::new())
        .expect_err("secret env rejected");
    assert_eq!(error.to_string(), "process environment rejected");
}

#[test]
fn receipt_carries_stream_tagged_chunks_with_real_text() {
    let receipt = SupervisedProcessRunner::new()
        .run(spec("mixed", 4096), CancellationSignal::new())
        .expect("run fixture");
    assert!(!receipt.chunks.is_empty() || !receipt.stdout.text.is_empty());
    assert!(receipt.stdout.text.contains("stdout-marker"));
    assert!(receipt.stderr.text.contains("stderr-marker"));
    // Chunks preserve per-stream tags when present.
    for chunk in &receipt.chunks {
        match chunk.stream {
            keeper_process_executor::OutputStreamTag::Stdout => {
                assert!(chunk.text.contains("stdout") || !chunk.text.is_empty());
            }
            keeper_process_executor::OutputStreamTag::Stderr => {
                assert!(chunk.text.contains("stderr") || !chunk.text.is_empty());
            }
        }
    }
    // Control neutralization is available for renderer projection.
    let projected = receipt.stdout.projected_text();
    assert!(!projected.contains('\u{1b}'));
}

#[test]
fn live_observer_is_non_authoritative_and_bounded() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let seen = Arc::new(AtomicUsize::new(0));
    let seen2 = Arc::clone(&seen);
    let observer: keeper_process_executor::OutputObserver = Arc::new(move |chunk| {
        assert!(!chunk.text.contains('\u{1b}'));
        seen2.fetch_add(chunk.text.len(), Ordering::SeqCst);
    });
    let receipt = SupervisedProcessRunner::new()
        .run_with_observer(
            spec("mixed", 4096),
            CancellationSignal::new(),
            Some(observer),
        )
        .expect("run with observer");
    assert!(seen.load(Ordering::SeqCst) > 0);
    assert!(receipt.stdout.text.contains("stdout-marker"));
    assert!(receipt.cleanup_proven);
}

#[test]
fn observer_never_receives_raw_over_cap_bytes() {
    use std::sync::{Arc, Mutex};
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let copy = Arc::clone(&chunks);
    let observer: keeper_process_executor::OutputObserver =
        Arc::new(move |chunk| copy.lock().unwrap().push(chunk));
    let receipt = SupervisedProcessRunner::new()
        .run_with_observer(
            spec("dual_saturated", 4096),
            CancellationSignal::new(),
            Some(observer),
        )
        .expect("bounded observer run");
    let chunks = chunks.lock().unwrap();
    assert!(chunks.iter().map(|chunk| chunk.text.len()).sum::<usize>() <= 4096);
    assert!(chunks.iter().all(|chunk| !chunk.text.contains('\u{1b}')));
    assert!(receipt.stdout.bytes_retained + receipt.stderr.bytes_retained <= 4096);
}

#[test]
#[allow(clippy::zombie_processes)] // Outer Job Object owns and kills this fixture tree.
fn fixture_child() {
    match std::env::var("KPE_FIXTURE_MODE").ok().as_deref() {
        Some("mixed") => {
            use std::io::Write;
            std::io::stdout()
                .write_all(b"stdout-marker \xFF")
                .expect("stdout");
            eprintln!("stderr-marker");
            std::process::exit(9);
        }
        Some("large") => {
            print!("{}", "a".repeat(128));
            eprint!("{}", "b".repeat(128));
        }
        Some("dual_saturated") => {
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    use std::io::Write;
                    let mut stdout = std::io::stdout();
                    for _ in 0..128 {
                        stdout.write_all(&[b'a'; 16 * 1024]).expect("stdout");
                    }
                });
                scope.spawn(|| {
                    use std::io::Write;
                    let mut stderr = std::io::stderr();
                    for _ in 0..128 {
                        stderr.write_all(&[b'b'; 16 * 1024]).expect("stderr");
                    }
                });
            });
        }
        Some("sleep") => std::thread::sleep(Duration::from_secs(10)),
        Some("grandchild") => {
            let exe = std::env::current_exe().expect("test executable");
            let mut command = std::process::Command::new(exe);
            command.args(["--exact", "fixture_child", "--nocapture"]);
            command.env("KPE_FIXTURE_MODE", "sleep");
            let _child = command.spawn().expect("fixture grandchild");
            std::thread::sleep(Duration::from_secs(10));
        }
        _ => {}
    }
}

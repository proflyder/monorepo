//! End-to-end check of the Rust scheduler client against the real Go
//! scheduler from this repository.
//!
//! It is `#[ignore]` because it needs a Go toolchain and the repository source
//! tree. Run it explicitly from the repo root:
//!
//! ```text
//! cargo test -p home-proto --test go_scheduler -- --ignored --nocapture
//! ```
//!
//! The test builds `./solutions/scheduler/cmd/scheduler` into a temporary
//! directory, starts it on a free loopback port with a temporary SQLite
//! database and an ephemeral token, then exercises enqueue → list_due (lease)
//! → ack → repeated ack (idempotent) and GetStatus. The child process is killed
//! when the test ends.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use home_proto::SchedulerClient;
use home_proto::v1::{AckOutcome, EnqueueTaskRequest, ListDueEventsRequest};
use home_secret::Token;

const CONSUMER: &str = "rust-integration";
const QUEUE: &str = "rusttest";
const TASK: &str = "test.ping";

struct GoScheduler {
    child: Child,
}

impl Drop for GoScheduler {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn repo_root() -> PathBuf {
    // packages/rust/home-proto -> packages/rust -> packages -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind free port");
    listener.local_addr().expect("addr").port()
}

async fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
#[ignore = "requires a Go toolchain and the scheduler source tree"]
async fn go_scheduler_enqueue_lease_ack_is_idempotent() {
    let root = repo_root();
    assert!(
        root.join("solutions/scheduler/cmd/scheduler/main.go")
            .is_file(),
        "scheduler sources not found under {}",
        root.display()
    );

    let port = free_port();
    let db_dir = tempfile::tempdir().expect("tempdir");
    let listen = format!("127.0.0.1:{port}");
    let token = format!("rust-integration-{}", std::process::id());

    // Build once and run the binary directly: `go run` is a wrapper, and
    // killing it would leave the real scheduler (and its stderr pipe) alive.
    let binary = db_dir.path().join("scheduler");
    let build = Command::new("go")
        .args([
            "build",
            "-o",
            binary.to_str().expect("utf8 path"),
            "./solutions/scheduler/cmd/scheduler",
        ])
        .current_dir(&root)
        .status()
        .expect("go build scheduler");
    assert!(build.success(), "go build scheduler failed: {build}");

    let child = Command::new(&binary)
        .current_dir(&root)
        .env("SCHEDULER_LISTEN", &listen)
        .env("SCHEDULER_DB_PATH", db_dir.path().join("scheduler.db"))
        .env("SCHEDULER_TOKEN", &token)
        .env("SCHEDULER_TIMEZONE", "UTC")
        .env("SCHEDULER_TICK_MS", "100")
        .env_remove("SCHEDULER_TOKEN_FILE")
        .env_remove("SCHEDULER_CONSUMERS")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("run scheduler");
    let _server = GoScheduler { child };

    assert!(
        wait_for_port(port, Duration::from_secs(120)).await,
        "scheduler did not start on {listen}"
    );

    let client = SchedulerClient::new(
        &format!("http://{listen}"),
        Token::new(token),
        Duration::from_secs(5),
    )
    .expect("client");

    let enqueued = client
        .enqueue_task(EnqueueTaskRequest {
            task_type: TASK.to_owned(),
            payload_json: r#"{"source":"rust-integration"}"#.to_owned(),
            queue: QUEUE.to_owned(),
            ..Default::default()
        })
        .await
        .expect("EnqueueTask");
    assert!(!enqueued.id.is_empty(), "empty event id");
    assert_eq!(enqueued.queue, QUEUE);
    println!(
        "enqueue: id={} queue={} task={} due={}",
        enqueued.id, enqueued.queue, enqueued.task_type, enqueued.due_unix
    );

    let events = client
        .list_due_events(ListDueEventsRequest {
            consumer_id: CONSUMER.to_owned(),
            lease_seconds: 60,
            limit: 10,
            queue: QUEUE.to_owned(),
            ..Default::default()
        })
        .await
        .expect("ListDueEvents");
    let event = events
        .iter()
        .find(|event| event.id == enqueued.id)
        .unwrap_or_else(|| panic!("enqueued event {} was not leased: {events:?}", enqueued.id));
    let event_id = event.id.clone();
    assert_eq!(event.task_type, TASK);
    assert!(event.lease_until_unix != 0, "event not leased");
    println!(
        "lease: id={} state={:?} attempts={} lease_until={}",
        event.id, event.state, event.attempts, event.lease_until_unix
    );

    let acked = client
        .ack_event(
            &event_id,
            CONSUMER,
            AckOutcome::ACK_OUTCOME_DELIVERED,
            "rust integration",
        )
        .await
        .expect("first AckEvent");
    assert_eq!(acked.id, event_id);
    println!("ack: id={} state={:?}", acked.id, acked.state);

    let replayed = client
        .ack_event(
            &event_id,
            CONSUMER,
            AckOutcome::ACK_OUTCOME_DELIVERED,
            "rust integration",
        )
        .await
        .expect("repeated AckEvent must be idempotent");
    assert_eq!(replayed.id, event_id);
    assert_eq!(
        replayed.state.to_i32(),
        acked.state.to_i32(),
        "replayed ack changed the terminal state"
    );
    println!("ack replay: id={} state={:?}", replayed.id, replayed.state);

    let status = client.get_status().await.expect("GetStatus");
    assert!(!status.version.is_empty(), "empty scheduler version");
    println!(
        "status: version={} schema={} pending={} leased={} ready={}",
        status.version,
        status.schema_version,
        status.pending_events,
        status.leased_events,
        status.ready
    );

    let listed_again = client
        .list_due_events(ListDueEventsRequest {
            consumer_id: CONSUMER.to_owned(),
            lease_seconds: 60,
            limit: 10,
            queue: QUEUE.to_owned(),
            ..Default::default()
        })
        .await
        .expect("ListDueEvents after ack");
    assert!(
        listed_again.iter().all(|event| event.id != event_id),
        "acked event was leased again"
    );
}

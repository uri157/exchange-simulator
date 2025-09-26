use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
    domain::models::dataset_status::DatasetStatus, infra::ingest::progress_sink::ProgressSink,
};

const BROADCAST_CAPACITY: usize = 64;

#[derive(Clone)]
struct CancellationFlag {
    inner: Arc<AtomicBool>,
}

impl CancellationFlag {
    fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    fn cancel(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetLogLine {
    pub line: String,
    pub ts: i64,
}

#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    pub status: DatasetStatus,
    pub progress: u8,
    pub last_message: Option<String>,
    pub updated_at: i64,
    pub logs: Vec<DatasetLogLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DatasetProgressEvent {
    Status {
        status: DatasetStatus,
        updated_at: i64,
    },
    Progress {
        progress: u8,
        last_message: Option<String>,
        updated_at: i64,
    },
    Log {
        line: String,
        ts: i64,
    },
    Done {
        status: DatasetStatus,
        updated_at: i64,
    },
    Error {
        status: DatasetStatus,
        last_message: Option<String>,
        updated_at: i64,
    },
}

impl DatasetProgressEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            DatasetProgressEvent::Status { .. } => "status",
            DatasetProgressEvent::Progress { .. } => "progress",
            DatasetProgressEvent::Log { .. } => "log",
            DatasetProgressEvent::Done { .. } => "done",
            DatasetProgressEvent::Error { .. } => "error",
        }
    }
}

#[derive(Clone)]
pub struct IngestionProgressRegistry {
    entries: Arc<RwLock<HashMap<Uuid, Arc<DatasetProgressEntry>>>>,
    log_capacity: usize,
}

impl IngestionProgressRegistry {
    pub fn new(log_capacity: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            log_capacity,
        }
    }

    pub fn bootstrap(
        &self,
        id: Uuid,
        status: DatasetStatus,
        updated_at: i64,
        last_message: Option<String>,
    ) {
        let entry = self.ensure_entry(id, status, updated_at);
        let mut state = entry.state.lock();
        state.status = status;
        state.updated_at = updated_at;
        state.progress = if matches!(status, DatasetStatus::Ready) {
            100
        } else {
            state.progress
        };
        state.last_message = last_message;
    }

    pub fn start_ingest(
        &self,
        id: Uuid,
        fallback_status: DatasetStatus,
    ) -> IngestionProgressHandle {
        let entry = self.ensure_entry(id, fallback_status, current_time_ms());
        entry.reset_for_ingest();
        let flag = entry.replace_flag();
        IngestionProgressHandle {
            dataset_id: id,
            registry: self.clone(),
            cancel_flag: flag,
        }
    }

    pub fn set_status(
        &self,
        id: Uuid,
        status: DatasetStatus,
        last_message: Option<String>,
    ) -> ProgressSnapshot {
        let entry = self.ensure_entry(id, status, current_time_ms());
        let snapshot = entry.update_status(status, last_message);
        entry.send_event(DatasetProgressEvent::Status {
            status,
            updated_at: snapshot.updated_at,
        });
        match status {
            DatasetStatus::Ready | DatasetStatus::Canceled => {
                entry.send_event(DatasetProgressEvent::Done {
                    status,
                    updated_at: snapshot.updated_at,
                });
            }
            DatasetStatus::Failed => {
                entry.send_event(DatasetProgressEvent::Error {
                    status,
                    last_message: snapshot.last_message.clone(),
                    updated_at: snapshot.updated_at,
                });
            }
            _ => {}
        }
        snapshot
    }

    pub fn set_progress(
        &self,
        id: Uuid,
        progress: u8,
        last_message: Option<String>,
    ) -> ProgressSnapshot {
        let entry = self.ensure_entry(id, DatasetStatus::Registered, current_time_ms());
        let snapshot = entry.update_progress(progress, last_message);
        entry.send_event(DatasetProgressEvent::Progress {
            progress: snapshot.progress,
            last_message: snapshot.last_message.clone(),
            updated_at: snapshot.updated_at,
        });
        snapshot
    }

    pub fn append_log(&self, id: Uuid, line: String) {
        let entry = self.ensure_entry(id, DatasetStatus::Registered, current_time_ms());
        let ts = current_time_ms();
        entry.append_log(line.clone(), ts);
        entry.send_event(DatasetProgressEvent::Log { line, ts });
    }

    pub fn snapshot_or_default(
        &self,
        id: Uuid,
        status: DatasetStatus,
        updated_at: i64,
    ) -> ProgressSnapshot {
        let entry = self.ensure_entry(id, status, updated_at);
        entry.snapshot()
    }

    pub fn subscribe(
        &self,
        id: Uuid,
        status: DatasetStatus,
        updated_at: i64,
    ) -> (ProgressSnapshot, broadcast::Receiver<DatasetProgressEvent>) {
        let entry = self.ensure_entry(id, status, updated_at);
        let snapshot = entry.snapshot();
        let rx = entry.events.subscribe();
        (snapshot, rx)
    }

    pub fn cancel(&self, id: Uuid) -> bool {
        let maybe_entry = {
            let read = self.entries.read();
            read.get(&id).cloned()
        };
        if let Some(entry) = maybe_entry {
            entry.cancel()
        } else {
            false
        }
    }

    pub fn clear(&self, id: Uuid) {
        self.entries.write().remove(&id);
    }

    fn ensure_entry(
        &self,
        id: Uuid,
        status: DatasetStatus,
        updated_at: i64,
    ) -> Arc<DatasetProgressEntry> {
        if let Some(existing) = self.entries.read().get(&id) {
            return existing.clone();
        }
        let mut write = self.entries.write();
        write
            .entry(id)
            .or_insert_with(|| {
                Arc::new(DatasetProgressEntry::new(
                    status,
                    updated_at,
                    self.log_capacity,
                ))
            })
            .clone()
    }
}

#[derive(Clone)]
pub struct IngestionProgressHandle {
    dataset_id: Uuid,
    registry: IngestionProgressRegistry,
    cancel_flag: CancellationFlag,
}

impl IngestionProgressHandle {
    pub fn dataset_id(&self) -> Uuid {
        self.dataset_id
    }

    pub fn registry(&self) -> &IngestionProgressRegistry {
        &self.registry
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_flag.is_cancelled()
    }
}

impl ProgressSink for IngestionProgressHandle {
    fn set_status(&self, status: DatasetStatus, last_message: Option<String>) {
        self.registry
            .set_status(self.dataset_id, status, last_message);
    }

    fn set_progress(&self, progress: u8, last_message: Option<String>) {
        self.registry
            .set_progress(self.dataset_id, progress, last_message);
    }

    fn append_log(&self, line: String) {
        self.registry.append_log(self.dataset_id, line);
    }

    fn is_cancelled(&self) -> bool {
        self.cancel_flag.is_cancelled()
    }
}

struct DatasetProgressEntry {
    state: Mutex<DatasetProgressState>,
    events: broadcast::Sender<DatasetProgressEvent>,
    cancel_flag: Mutex<CancellationFlag>,
    log_capacity: usize,
}

impl DatasetProgressEntry {
    fn new(status: DatasetStatus, updated_at: i64, log_capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            state: Mutex::new(DatasetProgressState::new(status, updated_at)),
            events: tx,
            cancel_flag: Mutex::new(CancellationFlag::new()),
            log_capacity,
        }
    }

    fn reset_for_ingest(&self) {
        let mut state = self.state.lock();
        state.progress = 0;
        state.last_message = None;
        state.logs.clear();
        state.updated_at = current_time_ms();
    }

    fn replace_flag(&self) -> CancellationFlag {
        let mut guard = self.cancel_flag.lock();
        let new_flag = CancellationFlag::new();
        *guard = new_flag.clone();
        new_flag
    }

    fn update_status(
        &self,
        status: DatasetStatus,
        last_message: Option<String>,
    ) -> ProgressSnapshot {
        let mut state = self.state.lock();
        state.status = status;
        if let Some(msg) = last_message {
            state.last_message = Some(msg);
        }
        if matches!(status, DatasetStatus::Ready) {
            state.progress = 100;
        }
        state.updated_at = current_time_ms();
        state.to_snapshot()
    }

    fn update_progress(&self, progress: u8, last_message: Option<String>) -> ProgressSnapshot {
        let mut state = self.state.lock();
        state.progress = progress.min(100);
        if let Some(msg) = last_message {
            state.last_message = Some(msg);
        }
        state.updated_at = current_time_ms();
        state.to_snapshot()
    }

    fn append_log(&self, line: String, ts: i64) {
        let mut state = self.state.lock();
        state.logs.push_back(DatasetLogLine { line, ts });
        while state.logs.len() > self.log_capacity {
            state.logs.pop_front();
        }
    }

    fn snapshot(&self) -> ProgressSnapshot {
        let state = self.state.lock();
        state.to_snapshot()
    }

    fn send_event(&self, event: DatasetProgressEvent) {
        let _ = self.events.send(event);
    }

    fn cancel(&self) -> bool {
        let guard = self.cancel_flag.lock();
        let already = guard.is_cancelled();
        guard.cancel();
        !already
    }
}

struct DatasetProgressState {
    status: DatasetStatus,
    progress: u8,
    last_message: Option<String>,
    updated_at: i64,
    logs: VecDeque<DatasetLogLine>,
}

impl DatasetProgressState {
    fn new(status: DatasetStatus, updated_at: i64) -> Self {
        Self {
            status,
            progress: if matches!(status, DatasetStatus::Ready) {
                100
            } else {
                0
            },
            last_message: None,
            updated_at,
            logs: VecDeque::new(),
        }
    }

    fn to_snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            status: self.status,
            progress: self.progress,
            last_message: self.last_message.clone(),
            updated_at: self.updated_at,
            logs: self.logs.iter().cloned().collect(),
        }
    }
}

fn current_time_ms() -> i64 {
    Utc::now().timestamp_millis()
}

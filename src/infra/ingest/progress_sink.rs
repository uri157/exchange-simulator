use crate::domain::models::dataset_status::DatasetStatus;

pub trait ProgressSink: Send + Sync {
    fn set_status(&self, status: DatasetStatus, last_message: Option<String>);
    fn set_progress(&self, progress: u8, last_message: Option<String>);
    fn append_log(&self, line: String);
    fn is_cancelled(&self) -> bool;
}

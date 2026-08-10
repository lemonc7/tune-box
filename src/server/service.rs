#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Resolving,
    Downloading,
    Tagging,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

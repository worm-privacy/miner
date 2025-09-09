use uuid::Uuid;
pub fn job_key(prefix:&str,id:Uuid) -> String {
    format!("{prefix}{id}")
}

pub fn now_millis() -> u64{
    use std::time::{SystemTime,UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else( |_| std::time::Duration::from_secs(0))
        .as_millis() as u64
}
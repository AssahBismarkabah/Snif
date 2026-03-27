const MAX_RETRIES: u32 = 3;
const TIMEOUT_SECS: u64 = 30;
const MAX_PAYLOAD_BYTES: usize = 10 * 1024 * 1024;

pub fn should_retry(attempt: u32) -> bool {
    attempt < MAX_RETRIES
}

pub fn is_payload_valid(size: usize) -> bool {
    size <= MAX_PAYLOAD_BYTES
}

pub fn timeout() -> std::time::Duration {
    std::time::Duration::from_secs(TIMEOUT_SECS)
}

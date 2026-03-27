use crate::crypto::hash_password;
use crate::crypto::verify_password;

pub fn authenticate(stored_hash: &str, password: &str) -> bool {
    verify_password(stored_hash, password)
}

pub fn register(password: &str) -> String {
    hash_password(password)
}

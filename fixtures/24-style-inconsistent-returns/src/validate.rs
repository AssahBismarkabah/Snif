pub fn is_valid_email(email: &str) -> bool {
    if email.is_empty() {
        return false;
    }
    let at_count = email.chars().filter(|c| *c == '@').count();
    if at_count != 1 {
        return false;
    }
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    !parts[0].is_empty() && !parts[1].is_empty() && parts[1].contains('.')
}

pub fn is_valid_port(port: u16) -> bool {
    port > 0
}

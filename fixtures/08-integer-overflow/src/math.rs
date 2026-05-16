pub fn factorial(n: u32) -> u64 {
    let mut result: u64 = 1;
    for i in 2..=n as u64 {
        result = result * i;
    }
    result
}

use std::thread;

pub fn count_parallel(n: usize) -> usize {
    let mut counter = 0usize;
    let handles: Vec<_> = (0..n)
        .map(|_| {
            thread::spawn(move || {
                counter += 1;
            })
        })
        .collect();
    for h in handles {
        let _ = h.join();
    }
    counter
}

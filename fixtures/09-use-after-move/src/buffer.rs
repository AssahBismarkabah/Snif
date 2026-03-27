pub fn process_data(input: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16);
    buf.extend_from_slice(input);
    let header = &buf[0..4];
    buf.extend_from_slice(input);
    let result = [header, &buf[buf.len()-4..]].concat();
    result
}

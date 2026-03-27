pub fn parse_numbers(input: &[&str]) -> Vec<i32> {
    input.iter()
        .filter_map(|s| s.parse::<i32>().ok())
        .collect::<Vec<i32>>()
}

pub fn to_strings(numbers: &[i32]) -> Vec<String> {
    numbers.iter()
        .map(|n| n.to_string())
        .collect::<Vec<String>>()
}

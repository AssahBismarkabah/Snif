pub struct Item { pub value: String }
pub struct Output;

#[derive(Debug)]
pub enum ProcessError {
    EmptyValue,
}

pub fn process_items(items: &[Item]) -> Vec<Result<Output, ProcessError>> {
    items.iter().map(|item| process_single(item)).collect()
}

fn process_single(item: &Item) -> Result<Output, ProcessError> {
    let validated = validate_item(item)?;
    Ok(transform(validated))
}

fn validate_item(item: &Item) -> Result<&Item, ProcessError> {
    if item.value.is_empty() {
        return Err(ProcessError::EmptyValue);
    }
    Ok(item)
}

fn transform(_item: &Item) -> Output { Output }

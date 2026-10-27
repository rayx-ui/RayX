use super::HistoryEntry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryLimits {
    pub max_entries_per_scope: usize,
    pub max_bytes_per_scope: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_entries_per_scope: 100,
            max_bytes_per_scope: 1024 * 1024,
        }
    }
}

pub fn encoded_size(entry: &HistoryEntry) -> usize {
    serde_json::to_vec(entry).map_or(usize::MAX, |bytes| bytes.len())
}

pub fn enforce(
    entries: &mut Vec<HistoryEntry>,
    limits: HistoryLimits,
) -> Result<usize, HistoryLimitError> {
    if entries
        .last()
        .is_some_and(|entry| encoded_size(entry) > limits.max_bytes_per_scope)
    {
        return Err(HistoryLimitError::EntryTooLarge);
    }
    let mut evicted = 0;
    while entries.len() > limits.max_entries_per_scope
        || entries.iter().map(encoded_size).sum::<usize>() > limits.max_bytes_per_scope
    {
        entries.remove(0);
        evicted += 1;
    }
    Ok(evicted)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryLimitError {
    EntryTooLarge,
}

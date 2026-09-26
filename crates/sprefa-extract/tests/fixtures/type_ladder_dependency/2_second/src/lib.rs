use ladder_collector::collector::{BulkTrigger, RowChange};

pub struct Second;

impl BulkTrigger for Second {
    fn on_batch(&mut self, _batch: &[RowChange]) {}
}

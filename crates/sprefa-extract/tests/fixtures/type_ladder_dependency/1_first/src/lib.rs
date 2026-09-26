use ladder_collector::collector::{BulkTrigger, RowChange};

pub struct First;

impl BulkTrigger for First {
    fn on_batch(&mut self, _batch: &[RowChange]) {}
}

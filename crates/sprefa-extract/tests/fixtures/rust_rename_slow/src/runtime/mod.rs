pub mod scheduler;
pub mod trace;

pub fn with_both(scheduler: &scheduler::Context, trace: &trace::Context) -> u32 {
    scheduler.id + trace.depth
}

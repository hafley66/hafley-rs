fn produce() -> i64 { 1 }
fn fallback() -> i64 { 0 }
fn consume(value: i64) {}
fn orchestrate(flag: bool) {
    let outcome = loop {
        if flag {
            break produce();
        }
        break fallback();
    };
    consume(outcome);
}

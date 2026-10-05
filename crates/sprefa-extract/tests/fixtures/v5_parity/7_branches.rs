fn produce() -> i64 { 1 }
fn fallback() -> i64 { 0 }
fn pick(k: i64) -> i64 { k }
fn consume(v: i64) {}
fn orchestrate(flag: bool, k: i64) {
    let x = if flag { produce() } else { fallback() };
    consume(x);
    let y = match k {
        0 => pick(k),
        _ => fallback(),
    };
    consume(y);
}

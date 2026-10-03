fn host(items: &[u32]) -> usize {
    let n = items.len();
    log(n);
    let f = |x: u32| x.count_ones();
    f(1);
    n
}

fn other() -> u32 {
    host(&[1, 2]);
    log(0);
    return 7;
}

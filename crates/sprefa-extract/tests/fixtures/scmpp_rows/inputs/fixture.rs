fn fact(n: u32) -> u32 {
    if n == 0 { 1 } else { n * fact(n - 1) }
}

fn helper() -> u32 {
    let seed = "seed";
    let total = fact(3);
    other();
    total + other()
}

fn other() -> u32 {
    let run = || other();
    run() + other()
}

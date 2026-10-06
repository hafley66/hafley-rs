fn parse_json(input: &str) {
    std::panic::catch_unwind(|| {
        may_throw();
        panic!("invalid JSON");
    });
}

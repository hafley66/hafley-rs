pub fn probe() {
    let text = "needle";
    let values = vec!["needle"];
    text.contains("needle");
    values.contains(&"needle");
    std::mem::drop(values);
}

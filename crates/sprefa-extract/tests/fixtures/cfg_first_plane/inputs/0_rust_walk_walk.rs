fn walk(items: Vec<i32>) -> i32 {
    let mut total = 0;
    for item in items {
        if item < 0 {
            continue;
        }
        if item > 100 {
            break;
        }
        total += item;
    }
    if total == 0 {
        return -1;
    }
    total
}

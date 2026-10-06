function walk(items: number[]): number {
  let total = 0;
  for (const item of items) {
    if (item < 0) {
      continue;
    }
    if (item > 100) {
      break;
    }
    total += item;
  }
  if (total === 0) {
    return -1;
  }
  return total;
}

function spin(limit: number): number {
  let seen = 0;
  do {
    seen += 1;
  } while (seen < limit);
  return seen;
}

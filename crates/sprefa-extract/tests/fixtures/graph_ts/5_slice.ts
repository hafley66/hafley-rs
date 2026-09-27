function choose(flag: boolean) {
  if (flag) {
    allow();
  } else {
    deny();
  }
  after();
}

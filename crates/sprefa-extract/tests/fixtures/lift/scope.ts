function outer() {
  var x: number = 1;
  const f = () => x;
  x = 2;
  return f();
}

class Box {
  value: number = 1;
  read() { return this.value; }
}

void outer;
void Box;

// node deframe.ts FILE -> one JSON message per line (Content-Length framed LSP log)
import fs from "node:fs";
let b = fs.readFileSync(process.argv[2]);
for (;;) {
  const h = b.indexOf("\r\n\r\n"); if (h < 0) break;
  const n = Number(/Content-Length: (\d+)/i.exec(b.subarray(0, h).toString())[1]);
  process.stdout.write(b.subarray(h + 4, h + 4 + n).toString() + "\n");
  b = b.subarray(h + 4 + n);
}

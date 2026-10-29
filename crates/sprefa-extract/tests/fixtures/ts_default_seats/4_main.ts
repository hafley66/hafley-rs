import Plain from "./0_plain";
import Wrapped from "./1_wrapped";
import Again, { helper } from "./2_reexport";
import { one, deux } from "./3_list";
import { MemoCard, MemoPlain } from "./6_const_memo";

export function main() {
  Plain();
  Wrapped();
  Again();
  helper();
  one();
  deux();
  MemoCard();
  MemoPlain();
}

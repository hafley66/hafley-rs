import { memo } from "./5_lib";
import Plain from "./0_plain";

function Card() {
  return null;
}
export const MemoCard = memo(Card);
export const MemoPlain = memo(Plain);
export const settings = Object.freeze({ dark: true });

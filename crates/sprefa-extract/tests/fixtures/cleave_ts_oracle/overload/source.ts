import { Code, code } from "./lib";
export { Code } from "./lib";

function wrap(value: string): Code;
function wrap(value: Code): Code;
function wrap(value: string | Code): Code {
  return typeof value === "string" ? code(value) : value;
}

export function twice(text: string): Code {
  return wrap(wrap(text));
}

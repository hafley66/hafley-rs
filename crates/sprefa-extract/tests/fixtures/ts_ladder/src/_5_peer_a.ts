export interface Peer { value: string }

function same(input: Peer): Peer {
  return input;
}

export function useA(input: Peer): Peer {
  return same(input);
}

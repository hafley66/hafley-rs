export interface Peer { value: string }

export function same(input: Peer): Peer {
  return input;
}

export function useB(input: Peer): Peer {
  return same(input);
}

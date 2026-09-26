export interface ExportedPeer { value: string }

export function sameExported(input: ExportedPeer): ExportedPeer {
  return input;
}

export function useC(input: ExportedPeer): ExportedPeer {
  return sameExported(input);
}

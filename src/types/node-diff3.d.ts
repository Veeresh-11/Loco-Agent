export interface NodeDiff3 {
  diff3(base: string, local: string, remote: string): string;
}

declare module 'node-diff3' {
  export const diff3: (base: string, local: string, remote: string) => string;
}

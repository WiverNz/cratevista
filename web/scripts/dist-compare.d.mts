// Type declarations for dist-compare.mjs (build tooling, not shipped).
export interface DistDiff {
  added: string[];
  removed: string[];
  changed: string[];
}
export declare function listFilesRecursive(dir: string, prefix?: string): string[];
export declare function compareDirs(baseline: string, candidate: string): DistDiff;
export declare function isIdentical(diff: DistDiff): boolean;
export declare function formatDiff(diff: DistDiff): string;
export declare function isDirectory(path: string): boolean;

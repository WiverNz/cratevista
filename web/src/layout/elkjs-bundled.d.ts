// Minimal type declaration for the browser-bundled elkjs entry (JS-only build).
declare module "elkjs/lib/elk.bundled.js" {
  export interface ElkLayoutArguments {
    layoutOptions?: Record<string, string>;
  }
  export default class ELK {
    constructor(options?: unknown);
    layout(graph: unknown, args?: ElkLayoutArguments): Promise<unknown>;
  }
}

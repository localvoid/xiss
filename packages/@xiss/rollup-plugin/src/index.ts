import type { Plugin } from "rollup";
import path from "node:path";

export const CSS_PATH = Symbol("xiss:path");

export function xiss(cssDir: string): Plugin {
  return {
    name: "xiss",
    async resolveId(source: string) {
      if (source.startsWith("xiss:")) {
        const modulePath = source.slice(5);
        return {
          id: path.join(cssDir, modulePath),
          moduleSideEffects: modulePath.endsWith(".css"),
        };
      }
      return;
    },
    transform(code: string, id: string) {
      if (id.endsWith(".css")) {
        return {
          code: "",
          moduleSideEffects: "no-treeshake",
          meta: { css: code },
        };
      }
      return;
    },
    renderChunk(_code: string, chunk) {
      let css = "";
      const modules = chunk.modules;
      for (const moduleId in modules) {
        if (moduleId.endsWith(".css")) {
          css += this.getModuleInfo(moduleId)!.meta.css;
        }
      }
      if (css === "") {
        return null;
      }
      const cssFile = this.emitFile({
        type: "asset",
        name: chunk.name + ".css",
        source: css,
      });
      (chunk as any)[CSS_PATH] = this.getFileName(cssFile);
      return;
    },
  };
}

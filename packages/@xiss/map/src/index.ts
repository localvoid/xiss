export type CssMap = Map<string, CssMapModule>;

export interface CssMapModule {
  classes: Map<string, CssMapId>;
  vars: Map<string, CssMapId>;
  keyframes: Map<string, CssMapId>;
}

export interface CssMapId {
  kind: "C" | "V" | "K";
  moduleId: string;
  localId: string;
  globalId: string;
}

export class CssMapError extends Error { }

export function createCssMap(): CssMap {
  return new Map();
}

export function importCssMapFromString(map: CssMap, contents: string) {
  const iter: Iter = {
    contents,
    i: 0,
    end: contents.length,
  };

  let kind: "C" | "V" | "K";
  while (iter.i < iter.end) {
    const c = contents.charCodeAt(iter.i);
    switch (c) {
      case 67: { // 'C'
        kind = "C";
        break;
      }
      case 86: { // 'V'
        kind = "V";
        break;
      }
      case 75: { // 'K'
        kind = "K";
        break;
      }
      default:
        throw new CssMapError(`Invalid kind '${c}'`);
    }
    // charCodeAt(iter.i+1) === 42 // 42 === ","
    iter.i += 2;

    const moduleId = readValueUntil(iter, 44); // 44 === ","
    const localId = readValueUntil(iter, 44); // 44 === ","
    const globalId = readValueUntil(iter, 10); // 10 === "\n"
    let m: CssMapModule | undefined;
    let prevModuleId: string | undefined;

    if (prevModuleId !== moduleId) {
      prevModuleId = moduleId;
      m = map.get(moduleId);
      if (m === void 0) {
        map.set(moduleId, m = {
          classes: new Map(),
          vars: new Map(),
          keyframes: new Map(),
        });
      }
    }
    const id: CssMapId = {
      kind,
      moduleId,
      localId,
      globalId,
    };
    switch (kind) {
      case "C":
        m!.classes.set(localId, id);
        break;
      case "V":
        m!.vars.set(localId, id);
        break;
      case "K":
        m!.keyframes.set(localId, id);
        break;
    }
  }
}

interface Iter {
  readonly contents: string;
  i: number;
  end: number;
}

function readValueUntil(iter: Iter, c: number): string {
  const contents = iter.contents;
  const start = iter.i;
  let i = start;
  while (iter.i < iter.end) {
    if (contents.charCodeAt(i) !== c) {
      iter.i = i + 1;
      return contents.substring(start, i);
    }
    i++;
  }
  throw new CssMapError("Unexpected end");
}

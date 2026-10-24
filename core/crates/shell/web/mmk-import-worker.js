// Import worker (see core/crates/shell/src/web_import.rs): runs the importer over the files the
// owner picked, reading them synchronously (FileReaderSync, workers only) and posting every
// output file to the page, which stores it. Nothing leaves the browser.
//
// Message in: { romfs: [[path under romfs/, File], ...], main: File (exefs/main) } for an
// extracted dump, or { nsps: [File, ...], keys: File (prod.keys) } for the dump itself (read in
// place by the importer; the keys only while it opens the NSPs). Runs the game data import, then
// the SMM2 UI import (its files posted as "@ui/<path>").
// Messages out: { write, bytes } per output file, { progress: [stage, done, total] },
// { done: summary JSON, ui: UI import summary JSON or null } or { error }.

import init, { import_dump, import_ui, open_nsps } from "./mmk-play.js";


self.onmessage = async ({ data: { romfs = [], main, nsps, keys } }) => {
  try {
    await init();
    const files = new Map(romfs.map(([p, f]) => [`/romfs/${p}`, f]));
    if (main) files.set("/exefs/main", main);
    // directory → names of its entries
    const dirs = new Map();
    for (const path of files.keys()) {
      const parts = path.split("/");
      for (let i = 1; i < parts.length - 1; i++) {
        const dir = parts.slice(0, i + 1).join("/");
        if (!dirs.has(dir)) dirs.set(dir, new Set());
        dirs.get(dir).add(parts[i + 1]);
      }
    }
    const reader = new FileReaderSync();
    let last = 0;
    globalThis.mmkImport = {
      read: (path) => {
        const f = files.get(path);
        return f ? new Uint8Array(reader.readAsArrayBuffer(f)) : null;
      },
      list: (dir) => [...(dirs.get(dir) ?? [])],
      isFile: (path) => files.has(path),
      write: (path, bytes) => postMessage({ write: path, bytes }, [bytes.buffer]),
      nspSize: (i) => nsps[i].size,
      nspRead: (i, offset, len) => new Uint8Array(reader.readAsArrayBuffer(nsps[i].slice(offset, offset + len))),
      progress: (stage, done, total) => {
        const now = performance.now();
        if (now - last > 200 || done === total) {
          last = now;
          postMessage({ progress: [stage, done, total] });
        }
      },
    };
    if (nsps) {
      postMessage({ progress: ["opening the NSP files", 0, 1] });
      open_nsps(nsps.map((f) => f.name), reader.readAsText(keys));
    }
    const summary = import_dump();
    // the SMM2 menus' import (written as "@ui/<path>"): optional, the game plays without it
    let ui = null;
    try {
      ui = import_ui();
    } catch (e) {
      console.warn("SMM2 UI import:", e);
    }
    postMessage({ done: summary, ui });
  } catch (e) {
    postMessage({ error: String(e?.message ?? e) });
  }
};

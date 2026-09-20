// Where a file that SHIPS WITH the app lives at runtime.
//
// The app reads four shipped resources: `prompt/system-prompt.md`,
// `data/prop-firms.json`, `data/copilot-notes.md` and `workflows/<id>/*`. In
// dev they sit in the working directory; in a packaged app they sit wherever
// electron-builder put them, and the three anchors Electron offers disagree:
//
//   process.cwd()          dev: the repo root, so an edit to workflows/ shows up
//                          without a rebuild. Packaged: whatever directory the
//                          shortcut points at (often C:\Windows\System32, or the
//                          user's home). A packaged app must therefore NEVER read
//                          a shipped resource relative to cwd — it would either
//                          find nothing or, worse, pick up a stranger's file.
//                          Hence `packaged` drops this candidate entirely.
//   app.getAppPath()       dev: the repo root. Packaged: <install>/resources/app.asar
//                          — this is where everything listed under `files:` in
//                          electron-builder.yml lands. Electron's fs layer reads
//                          straight through the archive, so this is the normal hit
//                          for a packaged build.
//   process.resourcesPath  packaged: <install>/resources (the directory that holds
//                          app.asar). The one anchor that is stable however the app
//                          was started, and where `extraResources:` or a
//                          hand-dropped override would land — so it stays last, as
//                          the escape hatch. Empty outside Electron (unit tests),
//                          so it is optional.
//
// Order: dev cwd -> app path -> resourcesPath. Electron-free on purpose, so the
// ordering rule can be unit-tested without booting an app.
import path from 'node:path';

export interface ResourceAnchors {
  /** process.cwd() */
  cwd: string;
  /** app.getAppPath() */
  appPath: string;
  /** process.resourcesPath ('' or null outside a packaged Electron app) */
  resourcesPath?: string | null;
  /** app.isPackaged — true drops the cwd candidate. */
  packaged: boolean;
}

/** The directories that may hold shipped resources, best candidate first. */
export function resourceRoots(a: ResourceAnchors): string[] {
  const roots: string[] = [];
  if (!a.packaged && a.cwd) roots.push(a.cwd);
  if (a.appPath) roots.push(a.appPath);
  if (a.resourcesPath) roots.push(a.resourcesPath);
  return [...new Set(roots)];
}

/** The same list, with a repo-relative path appended to every root. */
export function resourceCandidates(a: ResourceAnchors, ...rel: string[]): string[] {
  return resourceRoots(a).map((root) => path.join(root, ...rel));
}

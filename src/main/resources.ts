// The Electron-side wrapper around `./resource-paths`: it supplies the
// three anchors and reads the first candidate that exists.
//
// Every runtime read of a file that ships with the app goes through here, so
// there is exactly one place that knows the dev-cwd -> app.asar -> resourcesPath
// order and one place to look when a packaged build cannot find something.
import { app } from 'electron';
import { promises as fs } from 'node:fs';
import { resourceCandidates as candidatesFrom, resourceRoots as rootsFrom, type ResourceAnchors } from './resource-paths';

export function anchors(): ResourceAnchors {
  return {
    cwd: process.cwd(),
    appPath: app.getAppPath(),
    // Empty string in a dev run; `|| null` keeps it out of the candidate list.
    resourcesPath: process.resourcesPath || null,
    packaged: app.isPackaged,
  };
}

/** Directories that may hold shipped resources, best candidate first. */
export function resourceRoots(): string[] {
  return rootsFrom(anchors());
}

/** Full paths to one shipped resource, best candidate first. */
export function resourceCandidates(...rel: string[]): string[] {
  return candidatesFrom(anchors(), ...rel);
}

/** The text of the first candidate that reads, or null when none does. */
export async function readResource(...rel: string[]): Promise<string | null> {
  for (const p of resourceCandidates(...rel)) {
    try {
      return await fs.readFile(p, 'utf8');
    } catch {
      /* try the next anchor */
    }
  }
  return null;
}

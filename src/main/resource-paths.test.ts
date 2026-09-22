// The packaging bug this file exists to prevent: a resource that is found in
// dev because it sits in the working directory, and is gone the moment the app
// is installed somewhere else.
import { describe, expect, it } from 'vitest';
import path from 'node:path';
import { binaryCandidates, binaryRoots, resourceCandidates, resourceRoots, type ResourceAnchors } from './resource-paths';

const dev: ResourceAnchors = {
  cwd: path.join('C:', 'repo'),
  appPath: path.join('C:', 'repo'),
  resourcesPath: null,
  packaged: false,
};

const packaged: ResourceAnchors = {
  // A shortcut can start the app anywhere; System32 is the usual surprise.
  cwd: path.join('C:', 'Windows', 'System32'),
  appPath: path.join('C:', 'Apps', 'Strategy Desk', 'resources', 'app.asar'),
  resourcesPath: path.join('C:', 'Apps', 'Strategy Desk', 'resources'),
  packaged: true,
};

describe('resourceRoots', () => {
  it('in dev looks in the working directory first', () => {
    expect(resourceRoots({ ...dev, appPath: path.join('C:', 'elsewhere') })[0]).toBe(dev.cwd);
  });

  it('collapses cwd and appPath into one root when they are the same directory', () => {
    expect(resourceRoots(dev)).toEqual([path.join('C:', 'repo')]);
  });

  it('never reads a shipped file out of a packaged app cwd', () => {
    const roots = resourceRoots(packaged);
    expect(roots).not.toContain(packaged.cwd);
    expect(roots).toEqual([packaged.appPath, packaged.resourcesPath]);
  });

  it('keeps resourcesPath last, as the override/extraResources escape hatch', () => {
    expect(resourceRoots(packaged).at(-1)).toBe(packaged.resourcesPath);
  });

  it('drops an empty resourcesPath (unit tests, non-Electron hosts)', () => {
    expect(resourceRoots({ ...packaged, resourcesPath: '' })).toEqual([packaged.appPath]);
  });
});

describe('resourceCandidates', () => {
  it('finds the workflows and the rule book inside app.asar in a packaged build', () => {
    expect(resourceCandidates(packaged, 'workflows')).toEqual([
      path.join(packaged.appPath, 'workflows'),
      path.join(String(packaged.resourcesPath), 'workflows'),
    ]);
    expect(resourceCandidates(packaged, 'data', 'prop-firms.json')[0]).toBe(
      path.join(packaged.appPath, 'data', 'prop-firms.json'),
    );
  });

  it('resolves every shipped resource the app reads at runtime', () => {
    // If a new shipped resource appears, it belongs in electron-builder.yml's
    // `files:` as well — that is the pair this test keeps honest.
    for (const rel of [['prompt', 'system-prompt.md'], ['data', 'copilot-notes.md'], ['data', 'prop-firms.json'], ['workflows']]) {
      const c = resourceCandidates(packaged, ...rel);
      expect(c).toHaveLength(2);
      expect(c[0].startsWith(packaged.appPath)).toBe(true);
    }
  });
});

// A spawned process opens a path by name. app.asar is an archive that only
// Electron's own fs layer can read through, so offering a path inside it as a
// candidate would hand `uv` something that fails with ENOENT.
describe('binaryRoots', () => {
  it('never offers app.asar for something another process has to open', () => {
    const roots = binaryRoots(packaged);
    expect(roots).toEqual([packaged.resourcesPath]);
    expect(roots.some((r) => r.includes('app.asar'))).toBe(false);
  });

  it('still prefers the working directory in dev, so a local payload is picked up', () => {
    expect(binaryRoots(dev)).toEqual([dev.cwd]);
  });

  it('resolves the two things the installer carries next to app.asar', () => {
    expect(binaryCandidates(packaged, 'uv.exe')).toEqual([path.join(String(packaged.resourcesPath), 'uv.exe')]);
    expect(binaryCandidates(packaged, 'tvmcp', 'uv.lock')).toEqual([path.join(String(packaged.resourcesPath), 'tvmcp', 'uv.lock')]);
  });
});

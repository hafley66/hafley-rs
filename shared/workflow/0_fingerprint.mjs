import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Roots supply repository paths and selected inputs. Submodules contribute both
// their index pin and actual checkout, including dirty/untracked source files.
export function fingerprintSources(roots) {
  const hash = createHash('sha256');
  const git = (cwd, args) => execFileSync('git', args, { cwd, encoding: 'utf8' });
  function visit(cwd, paths) {
    const entries = git(cwd, ['ls-files', '--stage', '-z', '--', ...paths]).split('\0').filter(Boolean);
    const links = new Map();
    for (const entry of entries) {
      const tab = entry.indexOf('\t');
      const [mode, oid, stage] = entry.slice(0, tab).split(' ');
      if (mode === '160000') {
        if (stage !== '0') throw new Error(`Unmerged submodule: ${entry.slice(tab + 1)}`);
        links.set(entry.slice(tab + 1), oid);
      }
    }
    const files = git(cwd, ['ls-files', '-co', '--exclude-standard', '-z', '--', ...paths]).split('\0').filter(Boolean);
    for (const name of [...new Set(files)].sort()) {
      const path = resolve(cwd, name);
      hash.update(cwd + '\0' + name + '\0');
      if (links.has(name)) {
        hash.update('gitlink\0' + links.get(name) + '\0');
        if (existsSync(resolve(path, '.git'))) {
          hash.update(git(path, ['rev-parse', 'HEAD']));
          visit(path, ['.']);
        } else hash.update('<uninitialized>');
      } else {
        try { hash.update(readFileSync(path)); }
        catch (error) {
          if (error.code !== 'ENOENT') throw error;
          hash.update('<deleted>');
        }
      }
      hash.update('\0');
    }
  }
  for (const [cwd, paths] of roots) visit(cwd, paths);
  return hash.digest('hex');
}

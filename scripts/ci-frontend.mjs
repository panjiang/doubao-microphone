// CI-only fallback: keep checked-in registry settings and integrity values intact.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const web = fileURLToPath(new URL('../web/', import.meta.url));
const lockPath = new URL('../web/package-lock.json', import.meta.url);
const original = readFileSync(lockPath, 'utf8');
const run = (args) => {
  const result = spawnSync('npm', args, {
    cwd: web,
    stdio: 'inherit',
    // npm.cmd requires cmd.exe on Windows; arguments below are fixed constants.
    shell: process.platform === 'win32',
  });
  if (result.error) throw result.error;
  return result.status === 0;
};

try {
  if (!run(['ci', '--ignore-scripts', '--prefer-offline'])) {
    console.log('::warning::Primary npm install failed; retrying official npm with the same locked versions and integrity checks.');
    const lock = JSON.parse(original);
    let changed = 0;
    for (const pkg of Object.values(lock.packages)) {
      if (pkg.resolved?.startsWith('https://registry.npmmirror.com/')) {
        if (!pkg.integrity) throw new Error('Cannot fall back without a locked integrity value');
        pkg.resolved = pkg.resolved.replace('https://registry.npmmirror.com/', 'https://registry.npmjs.org/');
        changed++;
      }
    }
    if (!changed) throw new Error('No mirrored packages to retry');
    writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
    if (!run(['ci', '--ignore-scripts', '--prefer-offline', '--registry=https://registry.npmjs.org/'])) {
      throw new Error('Official npm fallback failed');
    }
  }
} finally {
  writeFileSync(lockPath, original);
}
if (!run(['run', 'build'])) throw new Error('Frontend build failed');

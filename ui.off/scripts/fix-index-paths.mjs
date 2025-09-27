// ui.off/scripts/fix-index-paths.mjs
import fs from 'node:fs';
import path from 'node:path';

const dist = path.resolve(process.cwd(), 'dist', 'index.html');
let html = fs.readFileSync(dist, 'utf8');

// force absolute /assets/ for both href and src
html = html.replace(/(href|src)="\/?assets\//g, '$1="/assets/');

fs.writeFileSync(dist, html, 'utf8');
console.log('[fix-index-paths] Rewrote asset URLs to /assets/');

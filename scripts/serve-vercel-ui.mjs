import { createServer } from 'node:http';
import { readFile, realpath, stat } from 'node:fs/promises';
import { dirname, extname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const config = JSON.parse(await readFile(resolve(root, 'vercel.json'), 'utf8'));
const service = config.services[process.argv[2]];
if (!service || service.runtime || !service.outputDirectory) throw new Error('Expected static service name');
const publicRoot = await realpath(resolve(root, service.root, service.outputDirectory));
const routes = service.routes.map(route => ({ ...route, pattern: route.src ? new RegExp(`^${route.src}$`) : undefined }));
const contentTypes = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.toml': 'text/plain', '.png': 'image/png', '.svg': 'image/svg+xml' };

// Local Vercel development only: production publishes files through @vercel/static-build.
createServer(async (request, response) => {
  try {
    if (!['GET', 'HEAD'].includes(request.method)) {
      response.writeHead(405).end();
      return;
    }
    const url = new URL(request.url, 'http://localhost');
    const pathname = decodeURIComponent(url.pathname);
    const load = async path => {
      let file = resolve(publicRoot, `.${path}`);
      if (file !== publicRoot && !file.startsWith(`${publicRoot}${sep}`)) throw new Error('Invalid static path');
      if ((await stat(file)).isDirectory()) file = resolve(file, 'index.html');
      file = await realpath(file);
      if (!file.startsWith(`${publicRoot}${sep}`)) throw new Error('Invalid static path');
      return { file, bytes: await readFile(file) };
    };
    const send = output => {
      response.writeHead(200, { 'content-type': contentTypes[extname(output.file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      response.end(request.method === 'HEAD' ? undefined : output.bytes);
    };
    for (const route of routes) {
      if (route.handle === 'filesystem') {
        try {
          send(await load(pathname));
          return;
        } catch {
          continue;
        }
      }
      if (!route.pattern?.test(pathname)) continue;
      if (route.status >= 300 && route.status < 400 && route.headers?.Location) {
        const location = pathname.replace(route.pattern, route.headers.Location);
        response.writeHead(route.status, { Location: `${location}${url.search}`, 'cache-control': 'no-store' }).end();
        return;
      }
      if (route.dest) {
        send(await load(pathname.replace(route.pattern, route.dest)));
        return;
      }
      response.writeHead(route.status || 404).end();
      return;
    }
    response.writeHead(404).end();
  } catch {
    response.writeHead(400).end();
  }
}).listen(Number(process.env.PORT), '127.0.0.1');

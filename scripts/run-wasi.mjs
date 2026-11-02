import { readFileSync, realpathSync, statSync } from 'node:fs';
import { WASI } from 'node:wasi';

const usage = 'Usage: node scripts/run-wasi.mjs [--dir /guest=HOST]... -- MODULE.wasm [ARG...]';
const args = process.argv.slice(2);
if (args.length === 1 && args[0] === '--help') {
  console.log(usage);
} else {
  try {
    const separator = args.indexOf('--');
    if (separator < 0 || !args[separator + 1]) throw new Error(usage);
    const preopens = Object.create(null);
    for (let index = 0; index < separator; index += 2) {
      if (args[index] !== '--dir' || index + 1 >= separator) throw new Error(usage);
      const mapping = args[index + 1];
      const equal = mapping.indexOf('=');
      if (equal < 2 || equal === mapping.length - 1) throw new Error('expected /guest=HOST');
      const guest = mapping.slice(0, equal);
      if (!guest.startsWith('/') || guest.slice(1).includes('/') || guest === '/.' || guest === '/..') {
        throw new Error('guest directories must be flat absolute paths such as /input');
      }
      if (Object.hasOwn(preopens, guest)) throw new Error(`duplicate guest directory: ${guest}`);
      const host = realpathSync(mapping.slice(equal + 1));
      if (!statSync(host).isDirectory()) throw new Error(`not a directory: ${host}`);
      preopens[guest] = host;
    }
    const modulePath = args[separator + 1];
    const wasi = new WASI({
      version: 'preview1',
      args: [modulePath, ...args.slice(separator + 2)],
      env: {},
      preopens,
      returnOnExit: true,
    });
    const module = new WebAssembly.Module(readFileSync(modulePath));
    const instance = new WebAssembly.Instance(module, wasi.getImportObject());
    process.exitCode = wasi.start(instance);
  } catch (error) {
    console.error(`run-wasi: ${error.message}`);
    process.exitCode = 1;
  }
}

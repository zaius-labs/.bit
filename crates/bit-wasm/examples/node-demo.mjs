// Usage: node examples/node-demo.mjs
// (Requires building WASM first: wasm-pack build --target nodejs)
import { parse, fmt, fromJson, fromMarkdown } from '../pkg/bit_lang_wasm.js';

console.log('--- Parse .bit ---');
const doc = parse(`# Users
define:@User
    name: ""!
    email: ""!

[!] Add authentication
[x] Set up database`);
console.log(JSON.stringify(doc, null, 2));

console.log('\n--- Format ---');
console.log(fmt('# Title\n[!] Task one\n[x] Done'));

console.log('\n--- From JSON ---');
console.log(fromJson('{"Product": {"name": "Widget", "price": 9.99}}'));

console.log('\n--- From Markdown ---');
console.log(fromMarkdown('# Tasks\n- [ ] Do thing\n- [x] Done'));

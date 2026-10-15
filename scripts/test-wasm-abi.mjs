import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { randomFillSync } from "node:crypto";

const bytes = await readFile(process.argv[2] ?? "target/wasm32-unknown-unknown/release/hegel_c.wasm");
let memory;
const { instance } = await WebAssembly.instantiate(bytes, {
  hegel_host: {
    entropy_fill(ptr, len) {
      randomFillSync(new Uint8Array(memory.buffer, ptr, len));
      return 1;
    },
    monotonic_nanos() {
      return process.hrtime.bigint();
    },
  },
});
const api = instance.exports;
memory = api.memory;
const allocations = [];
function allocate(size, align) {
  const pointer = api.hegel_alloc(size, align);
  assert.notEqual(pointer, 0);
  allocations.push([pointer, size, align]);
  return pointer;
}
const data = allocate(3, 1);
const string = allocate(8, 4);
const label = allocate(8, 8);
const error = allocate(8, 4);
new Uint8Array(memory.buffer, data, 3).set([97, 0, 98]);
function setString(pointer, length) {
  const view = new DataView(memory.buffer);
  view.setUint32(string, pointer, true);
  view.setUint32(string + 4, length, true);
}
const ctx = api.hegel_context_new();
assert.equal(api.hegel_label_from_name.length, 3);
assert.equal(api.hegel_context_last_error.length, 2);
setString(data, 3);
assert.equal(api.hegel_label_from_name(ctx, string, label), 0);
const full = new DataView(memory.buffer).getBigUint64(label, true);
setString(data, 1);
assert.equal(api.hegel_label_from_name(ctx, string, label), 0);
assert.notEqual(new DataView(memory.buffer).getBigUint64(label, true), full);
setString(0, 1);
assert.equal(api.hegel_label_from_name(ctx, string, label), -5);
api.hegel_context_last_error(error, ctx);
const view = new DataView(memory.buffer);
const message = new TextDecoder().decode(new Uint8Array(memory.buffer, view.getUint32(error, true), view.getUint32(error + 4, true)));
assert.match(message, /invalid string/);
assert.equal(new Uint8Array(memory.buffer)[view.getUint32(error, true) + view.getUint32(error + 4, true)], 0);
const slot = allocate(4, 4);
assert.equal(api.hegel_settings_new(ctx, slot), 0);
const settings = new DataView(memory.buffer).getUint32(slot, true);
setString(0, 0);
assert.equal(api.hegel_settings_set_database(ctx, settings, string), 0);
assert.equal(api.hegel_settings_get_database(ctx, settings, slot), 0);
const empty = new DataView(memory.buffer).getUint32(slot, true);
assert.notEqual(empty, 0);
const emptyData = new DataView(memory.buffer).getUint32(empty, true);
assert.notEqual(emptyData, 0);
assert.equal(new Uint8Array(memory.buffer)[emptyData], 0);
assert.equal(new DataView(memory.buffer).getUint32(empty + 4, true), 0);
assert.equal(api.hegel_settings_set_database(ctx, settings, 0), 0);
assert.equal(api.hegel_settings_get_database(ctx, settings, slot), 0);
assert.equal(new DataView(memory.buffer).getUint32(slot, true), 0);
assert.equal(api.hegel_settings_free(ctx, settings), 0);
assert.equal(api.hegel_context_free(ctx), 0);
for (const allocation of allocations) api.hegel_dealloc(...allocation);
console.log("Wasm string ABI checks passed");

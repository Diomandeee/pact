// Run the wasm and print its report. No npm, no wasm-bindgen: the boundary is
// a flat buffer, so the host side is the raw WebAssembly API and about thirty
// lines.
//
// Called by same_trace.rs with [wasmPath, specPath, seed, steps].

const [wasmPath, specPath, seedArg, stepsArg] = Deno.args;
const bytes = await Deno.readFile(wasmPath);
const spec = await Deno.readTextFile(specPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const { alloc, dealloc, run_buffer, memory } = instance.exports;

const input = new TextEncoder().encode(spec);
const inPtr = alloc(input.length);
new Uint8Array(memory.buffer, inPtr, input.length).set(input);

const outPtr = run_buffer(inPtr, input.length, BigInt(seedArg), Number(stepsArg));
const len = new DataView(memory.buffer).getUint32(outPtr, true);
const out = new TextDecoder().decode(
  new Uint8Array(memory.buffer, outPtr + 4, len),
);

dealloc(inPtr, input.length);
dealloc(outPtr, 4 + len);
// Raw, unformatted: the comparison is byte equality and re-serialising here
// would compare Deno's JSON formatter to serde's.
Deno.stdout.write(new TextEncoder().encode(out));

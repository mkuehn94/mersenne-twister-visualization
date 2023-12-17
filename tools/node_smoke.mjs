import fs from 'fs';
const bytes = fs.readFileSync(process.argv[2]);
const {instance} = await WebAssembly.instantiate(bytes, {});
const X = instance.exports;
const mem = () => X.memory.buffer;
const o32 = () => new Uint32Array(mem(), X.out32_ptr(), X.out32_len());
// load mt19937 config
X.variant_cfg(0); const cfg = o32().slice(); new Uint32Array(mem(), X.in_ptr(), 1024).set(cfg);
const h = X.vm_new_mt(5489, 0);
let t=performance.now();
X.vm_run(h, 3, 0, 0, 1e7); X.vm_outputs(h, 1); console.log('first', o32()[2], 'ms', performance.now()-t);
t=performance.now(); const n=X.vm_run(h, 8, 1000000, 0, 1e9); X.vm_outputs(h,1); console.log('1M outputs', n, 'steps', (performance.now()-t).toFixed(0),'ms');
t=performance.now(); const deg = X.la_charpoly(5489); console.log('deg', deg, 'terms', X.out32_len(), (performance.now()-t).toFixed(0),'ms');
const j = X.job_irr_new(0); t=performance.now(); while(!X.job_irr_step(j, 2000)){}; console.log('irr', o32().slice(), (performance.now()-t).toFixed(0),'ms');
X.mersenne_digits(19937); console.log('digits', X.outb_len(), new TextDecoder().decode(new Uint8Array(mem(), X.outb_ptr(), 12)));
const s = X.src_new(0, 1, 0); X.stat_uniform(s, 1e6, 100); console.log('chi2', new Float64Array(mem(), X.outf_ptr(), 3));

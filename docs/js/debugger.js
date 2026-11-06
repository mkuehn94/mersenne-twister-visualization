/* debugger.js — step-through UI for all the VMs (mt, tinymt, sfmt, melg, xsadd). */
(function () {
  "use strict";
  const MTV = window.MTV;
  const { h } = MTV;

  // step modes for the debugger transport (matches the JS-side button logic)
  const RUN = { STEP: 0, OVER: 1, OUT: 2, OUTPUT: 3, CONTINUE: 4, LOOP: 5, N: 6, DEEP: 7, OUTPUTS: 8 };
  MTV.RUN = RUN;
  // KaTeX is slow; cache rendered strings so we only typeset each formula once
  const texCache = new Map();
  const T = (s) => {
    let v = texCache.get(s);
    if (!v) { v = MTV.texStr(s); texCache.set(s, v); }
    return v;
  };

  function parseHeader(a) {
    const hd = {
      pc: a[0], depth: a[1], last: a[2], event: a[3],
      steps: a[4] + a[5] * 4294967296, outCount: a[6] + a[7] * 4294967296,
      write: a[8] | 0, reads: [a[9] | 0, a[10] | 0, a[11] | 0],
      rows: [], info: [],
    };
    let i = 12;
    const nt = a[i++];
    for (let r = 0; r < nt; r++, i += 6) hd.rows.push({ style: a[i], bits: a[i + 1], v: [a[i + 2], a[i + 3], a[i + 4], a[i + 5]] });
    const ni = a[i++];
    hd.info = Array.from(a.slice(i, i + ni));
    return hd;
  }
  function rowValue(r) {
    if (r.bits > 64) return (BigInt(r.v[3]) << 96n) | (BigInt(r.v[2]) << 64n) | (BigInt(r.v[1]) << 32n) | BigInt(r.v[0]);
    return MTV.j64(r.v[0], r.v[1]) & MTV.mask(r.bits);
  }
  function hexOf(v, bits) {
    if (bits > 64) {
      const s = v.toString(16).padStart(32, "0");
      return s.slice(0, 8) + " " + s.slice(8, 16) + " " + s.slice(16, 24) + " " + s.slice(24);
    }
    return MTV.hex(v, bits);
  }

  class Debugger {
    constructor(host, opts) {
      this.host = host;
      this.opts = Object.assign({ keys: true, seedLabel: "seed", outputs: true, watch: true }, opts);
      this.entries = (this.opts.entries || []).slice();
      this.bp = new Set();
      this.handle = 0;
      this.running = false;
      this.lastVal = new Map();
      this.pinned = -1;
      this.build();
      if (this.entries.length) this.load(this.entries[this.opts.initial || 0]);
    }

    // ------------------------------------------------------------ DOM
    build() {
      const o = this.opts;
      this.sel = h("select", { "aria-label": "program", onchange: () => this.load(this.entries[this.sel.selectedIndex]) });
      this.seedIn = h("input", { type: "text", class: "hex", size: "14", "aria-label": o.seedLabel, onchange: () => this.reset() });
      const btn = (label, key, fn, title) => h("button", { onclick: fn, title: title || "" }, label, key ? h("kbd", null, key) : null);
      this.runBtn = btn("Run", "Space", () => this.toggleRun(), "Animate at the chosen speed; stops at breakpoints");
      this.speed = h("input", { type: "range", min: "0", max: "100", value: o.speed || "38", "aria-label": "speed", oninput: () => this.showSpeed() });
      this.speedLab = h("span", { class: "note" });
      const want = o.buttons || ["step", "over", "out", "loop", "output", "continue", "bulk"];
      const B = {
        step: () => btn("Step", "F11", () => this.exec(RUN.STEP), "Execute the highlighted line"),
        over: () => btn("Step over", "F10", () => this.exec(RUN.OVER), "Execute the line; run called functions to completion"),
        out: () => btn("Step out", "⇧F11", () => this.exec(RUN.OUT), "Run until the current function returns"),
        loop: () => btn("Finish loop", "L", () => this.exec(RUN.LOOP), "Run until control leaves the current loop"),
        output: () => btn("Next number", "N", () => this.exec(RUN.OUTPUT), "Run until next() returns a number"),
        continue: () => btn("Continue", "F8", () => this.exec(RUN.CONTINUE), "Run to the next breakpoint (or the end)"),
        bulk: () => (o.bulk ? btn("+" + o.bulk + " numbers", null, () => this.exec(RUN.OUTPUTS, o.bulk), "Draw " + o.bulk + " numbers at full speed") : null),
      };
      const bar = h("div", { class: "toolbar" },
        this.entries.length > 1 || o.selector ? this.sel : null,
        h("label", { class: "inl" }, o.seedLabel, this.seedIn),
        btn("Reset", null, () => this.reset()),
        h("span", { class: "sep" }),
        ...want.map((k) => B[k]()),
        h("span", { class: "sep" }),
        this.runBtn, this.speed, this.speedLab
      );
      this.codeEl = h("div", { class: "code", role: "list" });
      this.stackEl = h("div", { class: "stack" });
      this.stateCv = h("canvas");
      this.stateTip = h("div", { class: "tip" });
      this.stateTitle = h("span");
      this.bitsEl = h("div", { class: "bits" });
      this.bitsTitle = h("span");
      const legend = h("div", { class: "legend" },
        h("span", { style: "--c: var(--read)" }, "read"),
        h("span", { style: "--c: var(--write)" }, "written"),
        h("span", { style: "--c: var(--write)" }, "rim: already twisted"),
        h("span", { style: "--c: var(--ink)" }, "word value: more ink = larger"));
      const codeP = h("div", { class: "panel code-panel" }, h("div", { class: "ptitle" }, h("span", null, "Pseudocode"), h("span", { class: "note" }, "click a line number for a breakpoint")), this.codeEl, this.stackEl);
      const stateP = (this.statePanel = h("div", { class: "panel" }, h("div", { class: "ptitle" }, this.stateTitle, h("span", { class: "note" }, "hover to read, click to pin")), h("div", { class: "ringwrap chart" }, this.stateCv, this.stateTip), legend));
      const bitsP = h("div", { class: "panel" }, h("div", { class: "ptitle" }, this.bitsTitle, h("span", { class: "note" }, "bit 31 … bit 0")), this.bitsEl);
      this.grid = h("div", { class: "dbg" }, codeP, stateP, bitsP);
      this.host.append(bar, this.grid);
      if (o.outputs || o.watch) {
        this.drawnEl = h("div", { class: "drawn" });
        this.logEl = h("div", { class: "outlog" });
        this.watchEl = h("div", { class: "watch" });
        const outP = h("div", { class: "panel" }, h("div", { class: "ptitle" }, h("span", null, "Drawn numbers")), this.drawnEl, this.logEl);
        const watchP = h("div", { class: "panel" }, h("div", { class: "ptitle" }, h("span", null, "Variables")), this.watchEl);
        this.host.append(h("div", { class: "cols wide-left", style: "margin-top:16px" }, o.outputs ? outP : null, o.watch ? watchP : null));
      }
      this.showSpeed();
      MTV.onResize(this.stateCv.parentElement, () => this.drawState());
      this.stateCv.addEventListener("mousemove", (e) => this.stateHover(e));
      this.stateCv.addEventListener("mouseleave", () => (this.stateTip.style.display = "none"));
      this.stateCv.addEventListener("click", (e) => {
        const i = this.stateIndexAt(e);
        if (i >= 0) { this.pinned = this.pinned === i ? -1 : i; this.renderBits(); this.drawState(); }
      });
      if (o.keys) this.bindKeys();
    }

    bindKeys() {
      this.visible = false;
      new IntersectionObserver((es) => es.forEach((e) => (this.visible = e.isIntersecting)), { threshold: 0.15 }).observe(this.grid);
      document.addEventListener("keydown", (e) => {
        if (!this.visible) return;
        const t = e.target;
        if (t && (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA")) return;
        let used = true;
        if (e.key === "F11" && e.shiftKey) this.exec(RUN.OUT);
        else if (e.key === "F11") this.exec(RUN.STEP);
        else if (e.key === "F10") this.exec(RUN.OVER);
        else if (e.key === "F8") this.exec(RUN.CONTINUE);
        else if (e.key === "n" || e.key === "N") this.exec(RUN.OUTPUT);
        else if (e.key === "l" || e.key === "L") this.exec(RUN.LOOP);
        else if (e.key === " ") this.toggleRun();
        else used = false;
        if (used) e.preventDefault();
      });
    }

    showSpeed() {
      const r = this.rate();
      this.speedLab.textContent = (r < 10 ? r.toFixed(1) : MTV.fmtInt(Math.round(r))) + " lines/s";
    }
    rate() {
      return Math.pow(10, (this.speed.value / 100) * 6.3);
    }

    // ------------------------------------------------------------ program management
    setEntries(list, idx) {
      this.entries = list.slice();
      this.sel.innerHTML = "";
      for (const e of this.entries) this.sel.append(h("option", null, e.label));
      if (idx != null) this.sel.selectedIndex = idx;
    }
    addEntry(entry, select) {
      const i = this.entries.findIndex((e) => e.label === entry.label);
      if (i >= 0) this.entries[i] = entry; else this.entries.push(entry);
      this.setEntries(this.entries, select ? (i >= 0 ? i : this.entries.length - 1) : this.sel.selectedIndex);
      if (select) this.load(entry);
    }
    load(entry) {
      if (!this.sel.options.length) this.setEntries(this.entries);
      const i = this.entries.indexOf(entry);
      if (i >= 0) this.sel.selectedIndex = i;
      this.entry = entry;
      this.prog = MTV.PROGS[entry.prog];
      this.cfg = entry.cfg || {};
      this.seedIn.value = entry.seedText != null ? entry.seedText : String(entry.seed != null ? entry.seed : 5489);
      this.bp.clear();
      (entry.breakpoints || []).forEach((b) => this.bp.add(b));
      this.renderCode();
      this.reset();
      if (entry.autoSeed && !entry._seeded) {
        entry._seeded = true;
        this.exec(RUN.OUT); // start with a filled state: seeding already done
      }
    }
    reset() {
      this.stop();
      if (this.handle) MTV.X.obj_free(this.handle);
      let s = MTV.parseBig(this.seedIn.value);
      if (s === null) { this.seedIn.classList.add("fail"); s = 0n; } else this.seedIn.classList.remove("fail");
      this.handle = this.prog.create(this.cfg, s);
      this.lastVal.clear();
      this.pinned = -1;
      this.refresh();
    }

    // ------------------------------------------------------------ execution
    bpMask() {
      let m = 0;
      for (const b of this.bp) if (b < 32) m |= 1 << b;
      return m >>> 0;
    }
    exec(mode, arg) {
      this.stop();
      MTV.X.vm_run(this.handle, mode, arg || 0, this.bpMask(), mode === RUN.STEP ? 1 : 50_000_000);
      this.refresh();
    }
    toggleRun() {
      if (this.running) return this.stop();
      this.running = true;
      this.runBtn.classList.add("on");
      this.runBtn.firstChild.textContent = "Pause";
      let acc = 0, last = performance.now();
      const tick = (now) => {
        if (!this.running) return;
        acc += (this.rate() * Math.min(100, now - last)) / 1000;
        last = now;
        const n = Math.floor(acc);
        if (n >= 1) {
          acc -= n;
          const done = MTV.X.vm_run(this.handle, RUN.CONTINUE, 0, this.bpMask(), n);
          this.refresh();
          if (done < n) return this.stop(); // breakpoint
        }
        this.raf = requestAnimationFrame(tick);
      };
      this.raf = requestAnimationFrame(tick);
    }
    stop() {
      this.running = false;
      cancelAnimationFrame(this.raf);
      if (this.runBtn) { this.runBtn.classList.remove("on"); this.runBtn.firstChild.textContent = "Run"; }
    }

    // ------------------------------------------------------------ views
    renderCode() {
      this.codeEl.innerHTML = "";
      this.lineEls = new Map();
      for (const L of this.prog.lines(this.cfg)) {
        const no = h("span", { class: "no" }, String(L.id));
        const bp = h("span", { class: "bp", title: L.exec ? "toggle breakpoint" : "" });
        const tx = h("span", { class: "tx", html: T(L.tex) });
        tx.style.paddingLeft = L.indent * 22 + "px";
        const val = h("span", { class: "val" });
        const el = h("div", { class: "ln" + (L.exec ? " exec" : " hdr") + (L.skip ? " skip" : ""), role: "listitem" }, bp, no, tx, val);
        if (L.exec) {
          const tog = () => {
            if (this.bp.has(L.id)) this.bp.delete(L.id); else this.bp.add(L.id);
            el.classList.toggle("bpon", this.bp.has(L.id));
          };
          bp.addEventListener("click", tog);
          no.addEventListener("click", tog);
          no.style.cursor = "pointer";
          if (this.bp.has(L.id)) el.classList.add("bpon");
        }
        this.lineEls.set(L.id, { el, val });
        this.codeEl.append(el);
      }
    }

    refresh() {
      MTV.X.vm_header(this.handle);
      this.hd = parseHeader(MTV.out32());
      const hd = this.hd;
      // remember the value computed by the executed line
      if (hd.rows.length) {
        const r = hd.rows[hd.rows.length - 1];
        this.lastVal.set(hd.last, r.bits <= 64 ? rowValue(r) : null);
      }
      for (const [id, { el, val }] of this.lineEls) {
        el.classList.toggle("pc", id === hd.pc);
        el.classList.toggle("last", id === hd.last);
        const txt = this.prog.inline(id, hd, hd.info, this.cfg, id === hd.last ? this.lastVal.get(id) : this.lastVal.get(id));
        val.textContent = id === hd.last || id === hd.pc ? txt : this.lastVal.has(id) ? txt : "";
      }
      const pcEl = this.lineEls.get(hd.pc);
      if (pcEl && this.opts.scrollCode !== false) {
        const box = this.codeEl;
        if (box.scrollHeight > box.clientHeight) pcEl.el.scrollIntoView({ block: "nearest" });
      }
      const fr = this.prog.frames(hd);
      this.stackEl.innerHTML = "call stack ";
      fr.forEach((f, i) => this.stackEl.append(h("span", { class: i === fr.length - 1 ? "top" : "" }, f + "()")));
      this.renderBits();
      this.drawState();
      if (this.opts.outputs) this.renderOutputs();
      if (this.opts.watch) this.renderWatch();
      if (this.opts.onRefresh) this.opts.onRefresh(this);
    }

    renderBits() {
      const hd = this.hd;
      const labels = this.prog.labels(hd.last, hd.rows.length, this.cfg) || [];
      const rd = (this.prog.readRows || {})[hd.last] || [];
      this.bitsEl.innerHTML = "";
      const lineNo = hd.last === 4294967295 ? null : hd.last;
      this.bitsTitle.innerHTML = lineNo == null ? "Bit inspector" : "Bit inspector · line " + lineNo;
      const addRow = (label, v, bits, cls, onBit, ref) => {
        const cells = h("div", { class: "cells" });
        const cols = Math.min(bits, 64);
        cells.style.gridTemplateColumns = "repeat(" + cols + ", minmax(0, 1fr))";
        for (let b = bits - 1; b >= 0; b--) {
          const one = (v >> BigInt(b)) & 1n;
          const i = h("i", { class: one ? "one" : "" });
          if (ref !== undefined && ((ref >> BigInt(b)) & 1n) !== one) i.classList.add("chg");
          if (onBit) { i.style.cursor = "pointer"; i.title = "bit " + b + " (click to flip)"; i.addEventListener("click", () => onBit(b)); }
          else i.title = "bit " + b;
          cells.append(i);
        }
        const row = h("div", { class: "row " + cls }, h("span", { class: "lab", html: T(label) }), cells, h("span", { class: "hex" }, hexOf(v, bits)));
        this.bitsEl.append(row);
      };
      if (this.pinned >= 0) {
        const pv = this.stateWord(this.pinned);
        if (pv) {
          const sn = this.prog.stateName || (this.prog.key === "tiny" ? "s" : "x");
          const isLung = this.prog.key === "melg" && this.pinned === this.cfg.nn;
          addRow(isLung ? "\\ell" : sn + "_{" + this.pinned + "}", pv.v, pv.bits, "s0 rd", (b) => {
            const nv = pv.v ^ (1n << BigInt(b));
            if (this.prog.setWord) this.prog.setWord(this, this.pinned, nv, b);
            else MTV.X.vm_set_word(this.handle, this.pinned, MTV.lo(nv), MTV.hi(nv));
            this.refresh();
          });
          this.bitsEl.append(h("div", { class: "note", style: "margin:0 0 8px 98px" }, "pinned word: click a bit to flip it in the live state (go on, break it)"));
        }
      }
      if (!hd.rows.length) {
        this.bitsEl.append(h("div", { class: "empty" }, lineNo == null ? "Hit Step to run the highlighted line." : "This line only moves control flow, no bits change here."));
        return;
      }
      const first = hd.rows[0];
      const fv = rowValue(first);
      hd.rows.forEach((r, i) => {
        const last = i === hd.rows.length - 1;
        if (last && hd.rows.length > 1 && r.style === 3) this.bitsEl.append(h("div", { class: "row rule" }, h("span"), h("div", { class: "cells" }), h("span")));
        const pw = this.prog.width(this.cfg);
        if (r.bits <= 64 && r.bits > pw) r = Object.assign({}, r, { bits: pw }); // e.g. a 64-bit seed shown at word size
        const v = rowValue(r);
        const cls = "s" + r.style + (rd.includes(i) ? " rd" : "");
        const ref = last && r.style === 3 && r.bits === first.bits && hd.rows.length > 1 ? fv : undefined;
        addRow(labels[i] || "", v, r.bits, cls, null, ref);
      });
    }

    stateWord(i) {
      if (this.prog.stateWord) return this.prog.stateWord(this, i);
      const X = MTV.X, key = this.prog.key;
      if (key === "mt") {
        const n = X.vm_state_len(this.handle);
        if (i >= n) return null;
        const a = MTV.u32at(X.vm_state_ptr(this.handle), 2 * n);
        return { v: MTV.j64(a[2 * i], a[2 * i + 1]), bits: this.cfg.w };
      }
      if (key === "tiny") return { v: BigInt(this.hd.info[i] >>> 0), bits: 32 };
      return null;
    }

    stateModel() {
      // returns {n, w, val(i) in [0,1], word(i) BigInt, kind:'ring'|'reg', touch(i), cur, fresh(i)}
      if (this.prog.stateModel) return this.prog.stateModel(this);
      const X = MTV.X, key = this.prog.key, hd = this.hd;
      if (key === "mt") {
        const n = X.vm_state_len(this.handle), w = this.cfg.w;
        const a = MTV.u32at(X.vm_state_ptr(this.handle), 2 * n);
        const k = hd.info[1], inTwist = hd.pc >= 14 && hd.pc <= 18;
        return {
          n, w, kind: n > 40 ? "ring" : "reg", name: "x",
          word: (i) => MTV.j64(a[2 * i], a[2 * i + 1]),
          frac: (i) => (w >= 32 ? (w > 32 ? a[2 * i + 1] : a[2 * i]) / 4294967296 : a[2 * i] / Math.pow(2, w)),
          fresh: (i) => inTwist && i < k,
          ptr: inTwist ? -1 : hd.info[2],
          chords: inTwist ? [[k % n, (k + 1) % n], [k % n, (k + this.cfg.m) % n]] : [],
          title: n + " words × " + w + " bits",
          bits: w,
        };
      }
      if (key === "tiny") {
        const s = hd.info;
        return { n: 4, w: 32, kind: "reg", name: "s", word: (i) => BigInt(s[i] >>> 0), frac: (i) => s[i] / 4294967296, fresh: () => false, ptr: -1, chords: [], title: "4 words, 127 effective bits", bits: 32 };
      }
      return null;
    }

    drawState() {
      if (!this.hd) return;
      const st = this.stateModel();
      const cv = this.stateCv;
      if (!st) { this.statePanel.style.display = "none"; this.grid.classList.add("nostate"); return; }
      this.statePanel.style.display = "";
      this.grid.classList.remove("nostate");
      this.stateTitle.textContent = "State · " + st.title;
      const ink = MTV.css("--ink"), bit0 = MTV.css("--bit0"), read = MTV.css("--read"), write = MTV.css("--write"),
        wsoft = MTV.css("--write-soft"), ink3 = MTV.css("--ink-3"), panel = MTV.css("--panel");
      const hd = this.hd;
      const reads = new Set(hd.reads.filter((r) => r >= 0));
      const wr = hd.write;
      this.geom = null;
      if (st.kind === "ring") {
        const { ctx, w, h: H } = MTV.fitCanvas(cv, Math.min(420, Math.max(300, cv.clientWidth * 0.82)));
        ctx.clearRect(0, 0, w, H);
        const cx = w / 2, cy = H / 2, R = Math.min(w, H) / 2 - 22, r0 = R * 0.8;
        this.geom = { cx, cy, R, r0, n: st.n };
        const ang = (i) => -Math.PI / 2 + (2 * Math.PI * i) / st.n;
        const da = (2 * Math.PI) / st.n;
        for (let i = 0; i < st.n; i++) {
          const a0 = ang(i) - da / 2 + 0.0015, a1 = ang(i) + da / 2 - 0.0015;
          ctx.beginPath();
          ctx.arc(cx, cy, R, a0, a1);
          ctx.arc(cx, cy, r0, a1, a0, true);
          ctx.closePath();
          let fill;
          if (wr === i || wr === -2) fill = write;
          else if (reads.has(i)) fill = read;
          else if (st.fresh(i)) fill = wsoft;
          else fill = bit0;
          ctx.fillStyle = fill;
          ctx.fill();
          if (fill === bit0 || fill === wsoft) {
            // value as ink density over the whole cell; twisted cells keep an amber rim
            ctx.globalAlpha = 0.08 + 0.8 * st.frac(i);
            ctx.fillStyle = ink;
            ctx.beginPath();
            ctx.arc(cx, cy, fill === wsoft ? R - (R - r0) * 0.22 : R, a0, a1);
            ctx.arc(cx, cy, r0, a1, a0, true);
            ctx.closePath();
            ctx.fill();
            ctx.globalAlpha = 1;
            if (fill === wsoft) {
              ctx.fillStyle = write;
              ctx.beginPath(); ctx.arc(cx, cy, R, a0, a1); ctx.arc(cx, cy, R - (R - r0) * 0.18, a1, a0, true); ctx.closePath(); ctx.fill();
            }
          }
          if (i === this.pinned) {
            ctx.strokeStyle = read; ctx.lineWidth = 2;
            ctx.beginPath(); ctx.arc(cx, cy, R + 5, a0 - da, a1 + da); ctx.stroke();
          }
        }
        // emphasise the cells touched by the last line with ticks outside the ring
        const tick = (i, color, len) => {
          const a = ang(i);
          ctx.strokeStyle = color; ctx.lineWidth = Math.max(3, (2 * Math.PI * R) / st.n);
          ctx.beginPath(); ctx.moveTo(cx + (R + 2) * Math.cos(a), cy + (R + 2) * Math.sin(a)); ctx.lineTo(cx + (R + 2 + len) * Math.cos(a), cy + (R + 2 + len) * Math.sin(a)); ctx.stroke();
        };
        reads.forEach((i) => i < st.n && tick(i, read, 9));
        if (wr >= 0 && wr < st.n) tick(wr, write, 13);
        // chords
        ctx.lineWidth = 1.5;
        st.chords.forEach(([a, b], j) => {
          ctx.strokeStyle = j === 0 ? read : read;
          ctx.globalAlpha = j === 0 ? 1 : 0.65;
          const pa = [cx + r0 * Math.cos(ang(a)), cy + r0 * Math.sin(ang(a))];
          const pb = [cx + r0 * Math.cos(ang(b)), cy + r0 * Math.sin(ang(b))];
          ctx.beginPath(); ctx.moveTo(pa[0], pa[1]); ctx.quadraticCurveTo(cx + (pa[0] + pb[0] - 2 * cx) * 0.25, cy + (pa[1] + pb[1] - 2 * cy) * 0.25, pb[0], pb[1]); ctx.stroke();
          ctx.globalAlpha = 1;
        });
        // pointer (next output index)
        if (st.ptr >= 0 && st.ptr < st.n) {
          const a = ang(st.ptr);
          ctx.fillStyle = ink;
          ctx.beginPath();
          ctx.moveTo(cx + (R + 4) * Math.cos(a), cy + (R + 4) * Math.sin(a));
          ctx.lineTo(cx + (R + 14) * Math.cos(a - 0.03), cy + (R + 14) * Math.sin(a - 0.03));
          ctx.lineTo(cx + (R + 14) * Math.cos(a + 0.03), cy + (R + 14) * Math.sin(a + 0.03));
          ctx.fill();
        }
        // index ticks
        ctx.fillStyle = ink3; ctx.font = "11px " + MTV.css("--sans"); ctx.textAlign = "center"; ctx.textBaseline = "middle";
        for (const q of [0, 0.25, 0.5, 0.75]) {
          const i = Math.round(q * st.n), a = ang(i);
          ctx.fillText(String(i), cx + (R + 17) * Math.cos(a) + (q === 0 ? 14 : 0), cy + (R + 17) * Math.sin(a));
        }
        // centre
        ctx.fillStyle = ink; ctx.textAlign = "center";
        ctx.font = "20px " + MTV.css("--serif");
        const center = this.centerText(st);
        ctx.fillText(center[0], cx, cy - 12);
        ctx.font = "13px " + MTV.css("--mono");
        ctx.fillStyle = MTV.css("--ink-2");
        ctx.fillText(center[1], cx, cy + 14);
        if (center[2]) { ctx.font = "12px " + MTV.css("--sans"); ctx.fillStyle = ink3; ctx.fillText(center[2], cx, cy + 36); }
      } else {
        // register view: one row per word, one column per bit
        const cols = st.w, rows = st.n;
        const wpx = cv.clientWidth || 400;
        const labW = 40;
        const cell = Math.max(3, Math.min(14, Math.floor((wpx - labW - 70) / cols)));
        const rowH = Math.max(6, Math.min(18, cell + 4));
        const { ctx, w } = MTV.fitCanvas(cv, rows * rowH + 8);
        ctx.clearRect(0, 0, w, rows * rowH + 8);
        this.geom = { reg: true, labW, cell, rowH, rows };
        ctx.font = Math.min(12, rowH - 2) + "px " + MTV.css("--sans");
        ctx.textBaseline = "middle";
        for (let i = 0; i < rows; i++) {
          const y = 4 + i * rowH;
          const v = st.word(i);
          const col = wr === i || wr === -2 ? write : reads.has(i) ? read : st.fresh(i) ? write : ink;
          ctx.fillStyle = ink3; ctx.textAlign = "right";
          ctx.fillText(st.label ? st.label(i) : st.name + i, labW - 6, y + rowH / 2);
          if (wr === i || reads.has(i) || i === this.pinned) {
            ctx.fillStyle = wr === i ? wsoft : MTV.css("--read-soft");
            ctx.fillRect(labW - 2, y, cols * cell + 4, rowH - 1);
          }
          for (let b = 0; b < cols; b++) {
            const one = (v >> BigInt(cols - 1 - b)) & 1n;
            ctx.fillStyle = one ? col : bit0;
            ctx.fillRect(labW + b * cell, y + (rowH - cell) / 2, cell - 1, cell - 1);
          }
          ctx.textAlign = "left"; ctx.fillStyle = MTV.css("--ink-2"); ctx.font = Math.min(12, rowH - 2) + "px " + MTV.css("--mono");
          if (rowH >= 12) ctx.fillText(MTV.hex(v, cols), labW + cols * cell + 8, y + rowH / 2);
          ctx.font = Math.min(12, rowH - 2) + "px " + MTV.css("--sans");
          if (st.ptr === i) { ctx.fillStyle = ink; ctx.fillText("◀", labW + cols * cell + (rowH >= 12 ? 90 : 8), y + rowH / 2); }
        }
      }
    }

    centerText(st) {
      if (this.prog.centerText) return this.prog.centerText(this, st);
      const hd = this.hd, key = this.prog.key;
      if (key === "mt") {
        const pc = hd.pc;
        if (pc <= 4) return ["seeding", "i = " + hd.info[0], ""];
        if (pc >= 14 && pc <= 18) return ["twist · k = " + hd.info[1], MTV.hex(st.word(hd.info[1] % st.n), st.w).slice(0, 20), "twist #" + (hd.info[9] + 1)];
        return ["idx = " + hd.info[2], hd.info[2] < st.n ? MTV.hex(st.word(hd.info[2]), st.w).slice(0, 20) : "next(): twist", hd.info[9] === 1 ? "1 twist done" : hd.info[9] + " twists done"];
      }
      return ["", "", ""];
    }

    stateIndexAt(e) {
      const g = this.geom;
      if (!g) return -1;
      const r = this.stateCv.getBoundingClientRect();
      const x = e.clientX - r.left, y = e.clientY - r.top;
      if (g.reg) {
        const i = Math.floor((y - 4) / g.rowH);
        return i >= 0 && i < g.rows ? i : -1;
      }
      const dx = x - g.cx, dy = y - g.cy, d = Math.hypot(dx, dy);
      if (d < g.r0 - 4 || d > g.R + 8) return -1;
      let a = Math.atan2(dy, dx) + Math.PI / 2;
      if (a < 0) a += 2 * Math.PI;
      return Math.round((a / (2 * Math.PI)) * g.n) % g.n;
    }
    stateHover(e) {
      const i = this.stateIndexAt(e);
      if (i < 0) { this.stateTip.style.display = "none"; return; }
      const st = this.stateModel();
      const r = this.stateCv.getBoundingClientRect();
      this.stateTip.innerHTML = (st.label && st.label(i) === "ℓ" ? "ℓ" : st.name + "<sub>" + i + "</sub>") + " = " + hexOf(st.word(i), st.bits);
      this.stateTip.style.display = "block";
      this.stateTip.style.left = e.clientX - r.left + "px";
      this.stateTip.style.top = e.clientY - r.top + "px";
    }

    renderOutputs() {
      MTV.X.vm_outputs(this.handle, 40);
      const a = MTV.out32();
      const recs = [];
      for (let i = 0; i + 7 <= a.length; i += 7) recs.push({ idx: a[i] + a[i + 1] * 4294967296, v: MTV.j64(a[i + 2], a[i + 3]), raw: MTV.j64(a[i + 4], a[i + 5]), disc: a[i + 6] & 1 });
      recs.reverse();
      const w = this.prog.width(this.cfg);
      const bits = this.prog.key === "mt" ? w - (this.cfg.outShift || 0) : w;
      this.drawnEl.innerHTML = "";
      if (!recs.length) {
        this.drawnEl.append(h("div", { class: "note", style: "grid-column: span 2" }, "Nothing drawn yet. “Next number” runs until next() returns."));
      } else {
        const r = recs[0];
        const bin = h("div", { class: "bin" });
        bin.style.gridTemplateColumns = "repeat(" + Math.min(bits, 64) + ", minmax(0,1fr))";
        for (let b = bits - 1; b >= 0; b--) bin.append(h("i", { class: (r.v >> BigInt(b)) & 1n ? "one" : "" }));
        const u = Number(r.v) / Math.pow(2, bits);
        this.drawnEl.append(
          h("span", { class: "k", html: T("z_{" + r.idx + "}") }), h("span", { class: "big" }, MTV.hex(r.v, bits)),
          h("span", { class: "k" }, "binary"), bin,
          h("span", { class: "k" }, "decimal"), h("span", { class: "dec" }, r.v.toString() + (r.disc ? "   (discarded)" : "")),
          h("span", { class: "k", html: T("z/2^{" + bits + "}") }), h("span", { class: "dec" }, u.toFixed(Math.min(17, Math.ceil(bits * 0.31) + 1)))
        );
      }
      const showBin = bits <= 32;
      const rowsHtml = recs.map((r) => "<tr" + (r.disc ? ' class="disc"' : "") + "><td>" + r.idx + "</td><td>" + MTV.hex(r.v, bits) + "</td>" + (showBin ? "<td>" + MTV.bin(r.v, bits) + "</td>" : "") + "<td>" + r.v.toString() + "</td></tr>").join("");
      this.logEl.innerHTML = recs.length ? "<table><tr><th>#</th><th>hex</th>" + (showBin ? "<th>binary</th>" : "") + "<th>decimal</th></tr>" + rowsHtml + "</table>" : "";
    }

    renderWatch() {
      const rows = this.prog.watch(this.hd.info, this.hd, this.cfg);
      this.watchEl.innerHTML = "";
      for (const [k, v] of rows) this.watchEl.append(h("span", { class: "k", html: T(k) }), h("span", { class: "v" }, String(v)));
    }
  }

  MTV.Debugger = Debugger;
})();

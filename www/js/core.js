/* core.js — wasm glue, config encode/decode, number formatting, canvas helpers.
   Plain script tag (no ES modules) so it works from file:// without a server. */
(function () {
  "use strict";
  const MTV = (window.MTV = window.MTV || {});

  // ------------------------------------------------------------------ wasm
  let X = null;
  MTV.load = async function () {
    let bytes;
    if (window.MTV_WASM_B64) {
      const s = atob(window.MTV_WASM_B64);
      bytes = new Uint8Array(s.length);
      for (var i = 0; i < s.length; i++) bytes[i] = s.charCodeAt(i);
    } else {
      bytes = await (await fetch("mtviz.wasm")).arrayBuffer();
    }
    const { instance } = await WebAssembly.instantiate(bytes, {});
    X = MTV.X = instance.exports;
    return X;
  };
  const mem = () => X.memory.buffer;
  MTV.out32 = () => new Uint32Array(mem(), X.out32_ptr(), X.out32_len());
  MTV.outf = () => new Float64Array(mem(), X.outf_ptr(), X.outf_len());
  MTV.outb = () => new Uint8Array(mem(), X.outb_ptr(), X.outb_len());
  MTV.u32at = (ptr, n) => new Uint32Array(mem(), ptr, n);

  // ------------------------------------------------------------------ 64-bit helpers
  // wasm gives us two u32s per 64-bit value; these pack/unpack to BigInt for display
  const B = BigInt;
  MTV.j64 = (lo, hi) => (B(hi >>> 0) << 32n) | B(lo >>> 0);
  MTV.lo = (x) => Number(B.asUintN(32, x));
  MTV.hi = (x) => Number(B.asUintN(32, x >> 32n));
  MTV.mask = (w) => (w >= 64 ? (1n << 64n) - 1n : (1n << B(w)) - 1n);

  MTV.hex = function (x, w) {
    const v = B.asUintN(64, B(x));
    const s = v.toString(16);
    return "0x" + s.padStart(Math.ceil(w / 4), "0");
  };
  MTV.bin = function (x, w) {
    return B.asUintN(64, B(x)).toString(2).padStart(w, "0");
  };
  MTV.parseBig = function (s) {
    s = String(s).trim().replace(/_/g, "");
    if (!s) return 0n;
    try {
      if (/^0b/i.test(s)) return B(s);
      if (/^0x/i.test(s)) return B(s);
      if (/^-/.test(s)) return B.asUintN(64, B(s));
      return B(s);
    } catch (e) {
      return null;
    }
  };

  // ------------------------------------------------------------------ configurations
  // MUST match the layout in crate/src/api.rs — if you add a field there, add it here too
  const F = ["w", "n", "m", "r", "a2", "u", "d2", "s", "b2", "t", "c2", "l", "f2", "umask2", "lmask2", "lowbit", "temper", "seedMode", "zeroSub2", "seedXor2", "outShift", "outXor2", "outAnd2", "discard"];
  MTV.decodeCfg = function (arr) {
    const c = {};
    let i = 0;
    for (const f of F) {
      if (f.endsWith("2")) {
        c[f.slice(0, -1)] = MTV.j64(arr[i], arr[i + 1]);
        i += 2;
      } else {
        c[f] = arr[i++];
      }
    }
    c.lowbit = !!c.lowbit;
    c.temper = !!c.temper;
    return c;
  };
  MTV.setCfg = function (c) {
    const buf = new Uint32Array(mem(), X.in_ptr(), 64);
    let i = 0;
    for (const f of F) {
      if (f.endsWith("2")) {
        const v = B.asUintN(64, B(c[f.slice(0, -1)]));
        buf[i++] = MTV.lo(v);
        buf[i++] = MTV.hi(v);
      } else {
        let v = c[f];
        if (typeof v === "boolean") v = v ? 1 : 0;
        buf[i++] = Number(v) >>> 0;
      }
    }
  };
  MTV.variant = function (id) {
    X.variant_cfg(id);
    return MTV.decodeCfg(MTV.out32());
  };
  MTV.cloneCfg = (c) => Object.assign({}, c);
  MTV.p = (c) => c.n * c.w - c.r;

  // ------------------------------------------------------------------ math rendering
  MTV.tex = function (el, src, display) {
    if (typeof el === "string") el = document.querySelector(el);
    if (!el) return;
    try {
      katex.render(src, el, { displayMode: !!display, throwOnError: false, strict: false, trust: true });
    } catch (e) {
      el.textContent = src;
    }
  };
  MTV.texStr = function (src, display) {
    try {
      return katex.renderToString(src, { displayMode: !!display, throwOnError: false, strict: false, trust: true });
    } catch (e) {
      return src;
    }
  };
  MTV.renderAllTex = function (root) {
    (root || document).querySelectorAll("[data-tex]").forEach((el) => {
      MTV.tex(el, el.getAttribute("data-tex"), el.classList.contains("eq") || el.hasAttribute("data-display"));
    });
  };
  MTV.texHex = (x, w) => "\\texttt{" + MTV.hex(x, w) + "}";

  // ------------------------------------------------------------------ DOM helpers
  MTV.$ = (s, r) => (r || document).querySelector(s);
  MTV.$$ = (s, r) => Array.from((r || document).querySelectorAll(s));
  MTV.h = function (tag, attrs, ...kids) {
    const e = document.createElement(tag);
    if (attrs)
      for (const k in attrs) {
        if (k === "class") e.className = attrs[k];
        else if (k === "html") e.innerHTML = attrs[k];
        else if (k.startsWith("on")) e.addEventListener(k.slice(2), attrs[k]);
        else e.setAttribute(k, attrs[k]);
      }
    for (const k of kids) if (k != null) e.append(k);
    return e;
  };
  MTV.css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  MTV.fmt = function (x, d) {
    if (!isFinite(x)) return String(x);
    if (d === undefined) d = 4;
    const a = Math.abs(x);
    if (a !== 0 && (a < 1e-3 || a >= 1e6)) return x.toExponential(2);
    return x.toFixed(d);
  };
  MTV.fmtInt = (x) => Number(x).toLocaleString("en-US");

  // theme change → redraw everything that registered
  const redraws = new Set();
  MTV.onTheme = (fn) => redraws.add(fn);
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener && mq.addEventListener("change", () => redraws.forEach((f) => f()));
  MTV.toggleTheme = function () {
    const root = document.documentElement;
    const dark = root.dataset.theme ? root.dataset.theme === "dark" : mq.matches;
    root.dataset.theme = dark ? "light" : "dark";
    try { localStorage.setItem("mtv-theme", root.dataset.theme); } catch (e) {}
    redraws.forEach((f) => f());
  };
  try {
    const t = localStorage.getItem("mtv-theme");
    if (t) document.documentElement.dataset.theme = t;
  } catch (e) {}

  // ------------------------------------------------------------------ canvas
  // size a canvas to its CSS width (and given aspect or height) at device resolution
  MTV.fitCanvas = function (cv, heightOrAspect) {
    const dpr = window.devicePixelRatio || 1;
    const w = cv.clientWidth || cv.parentElement.clientWidth || 300;
    const h = heightOrAspect < 10 ? Math.round(w * heightOrAspect) : heightOrAspect;
    cv.style.height = h + "px";
    if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
      cv.width = Math.round(w * dpr);
      cv.height = Math.round(h * dpr);
    }
    const ctx = cv.getContext("2d");
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { ctx, w, h };
  };
  MTV.onResize = function (el, fn) {
    let raf = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(fn);
    });
    ro.observe(el);
    MTV.onTheme(fn);
  };

  // draw a bit matrix (rows of 0/1 accessors) into a canvas, pixel-exact
  MTV.drawBitMatrix = function (cv, rows, cols, get, colorOne, colorZero, maxPx) {
    const scale = Math.max(1, Math.floor(Math.min(maxPx || 600, cv.parentElement.clientWidth || 600) / cols));
    cv.width = cols * scale;
    cv.height = rows * scale;
    cv.style.width = cols * scale + "px";
    cv.style.height = rows * scale + "px";
    cv.style.maxWidth = "100%";
    cv.style.height = "auto";
    const ctx = cv.getContext("2d");
    ctx.fillStyle = colorZero;
    ctx.fillRect(0, 0, cv.width, cv.height);
    ctx.fillStyle = colorOne;
    for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) if (get(r, c)) ctx.fillRect(c * scale, r * scale, scale, scale);
  };

  // ------------------------------------------------------------------ a small plot helper
  /**
   * new MTV.Plot(container, {height, xlabel, ylabel, logx, logy, xr:[a,b], yr:[a,b]})
   * plot.draw(layers) where layers = [{type:'bars'|'line'|'dots'|'band'|'hline', ...}]
   * hover: layers may carry `tip(i)` and points to support tooltips
   */
  class Plot {
    constructor(container, opt) {
      this.el = container;
      this.opt = Object.assign({ height: 260, pad: [14, 14, 38, 56] }, opt || {});
      this.cv = MTV.h("canvas");
      this.tip = MTV.h("div", { class: "tip" });
      container.classList.add("chart");
      container.append(this.cv, this.tip);
      this.layers = [];
      this.hits = [];
      this.cv.addEventListener("mousemove", (e) => this.hover(e));
      this.cv.addEventListener("mouseleave", () => { this.tip.style.display = "none"; this.hoverX = null; this.render(); });
      MTV.onResize(container, () => this.render());
    }
    sx(x) {
      const { xr, logx } = this.opt;
      const [l, , , ] = [this.L];
      const a = logx ? Math.log10(xr[0]) : xr[0], b = logx ? Math.log10(xr[1]) : xr[1];
      const v = logx ? Math.log10(x) : x;
      return this.L + ((v - a) / (b - a)) * (this.W - this.L - this.R);
    }
    sy(y) {
      const { yr, logy } = this.opt;
      const a = logy ? Math.log10(yr[0]) : yr[0], b = logy ? Math.log10(yr[1]) : yr[1];
      const v = logy ? Math.log10(Math.max(y, yr[0])) : y;
      return this.H - this.Bm - ((v - a) / (b - a)) * (this.H - this.T - this.Bm);
    }
    draw(layers, opt) {
      if (opt) Object.assign(this.opt, opt);
      this.layers = layers;
      this.render();
    }
    ticks(a, b, log, n) {
      if (log) {
        const out = [];
        for (let e = Math.ceil(Math.log10(a)); e <= Math.floor(Math.log10(b)); e++) out.push(Math.pow(10, e));
        return out;
      }
      const span = b - a;
      const step0 = span / (n || 5);
      const mag = Math.pow(10, Math.floor(Math.log10(step0)));
      const r = step0 / mag;
      const step = (r < 1.5 ? 1 : r < 3 ? 2 : r < 7 ? 5 : 10) * mag;
      const out = [];
      for (let v = Math.ceil(a / step) * step; v <= b + step * 1e-9; v += step) {
        // snap values that are basically zero so we dont print -0.0000
        if (Math.abs(v) < step * 1e-9) out.push(0); else out.push(v);
      }
      return out;
    }
    render() {
      const o = this.opt;
      const { ctx, w, h } = MTV.fitCanvas(this.cv, o.height);
      this.W = w; this.H = h;
      [this.T, this.R, this.Bm, this.L] = o.pad;
      const ink = MTV.css("--ink"), ink3 = MTV.css("--ink-3"), grid = MTV.css("--grid"), rule = MTV.css("--rule");
      ctx.clearRect(0, 0, w, h);
      ctx.font = "11px " + MTV.css("--sans");
      // grid + ticks
      ctx.strokeStyle = grid; ctx.fillStyle = ink3; ctx.lineWidth = 1;
      const xt = o.xticks || this.ticks(o.xr[0], o.xr[1], o.logx, Math.max(3, Math.floor(w / 90)));
      const yt = o.yticks || this.ticks(o.yr[0], o.yr[1], o.logy, 4);
      ctx.textAlign = "right"; ctx.textBaseline = "middle";
      for (const v of yt) {
        const y = Math.round(this.sy(v)) + 0.5;
        ctx.beginPath(); ctx.moveTo(this.L, y); ctx.lineTo(w - this.R, y); ctx.stroke();
        ctx.fillText(o.yfmt ? o.yfmt(v) : MTV.tickFmt(v), this.L - 6, y);
      }
      ctx.textAlign = "center"; ctx.textBaseline = "top";
      for (const v of xt) {
        const x = Math.round(this.sx(v)) + 0.5;
        ctx.fillText(o.xfmt ? o.xfmt(v) : MTV.tickFmt(v), x, h - this.Bm + 6);
      }
      ctx.strokeStyle = rule;
      ctx.beginPath(); ctx.moveTo(this.L, h - this.Bm + 0.5); ctx.lineTo(w - this.R, h - this.Bm + 0.5); ctx.stroke();
      // labels
      ctx.fillStyle = ink3; ctx.font = "12px " + MTV.css("--sans");
      if (o.xlabel) { ctx.textAlign = "right"; ctx.textBaseline = "bottom"; ctx.fillText(o.xlabel, w - this.R, h - 2); }
      if (o.ylabel) { ctx.textAlign = "left"; ctx.textBaseline = "top"; ctx.fillText(o.ylabel, this.L + 4, 0); }
      this.hits = [];
      ctx.save();
      ctx.beginPath(); ctx.rect(this.L, this.T - 2, w - this.L - this.R, h - this.T - this.Bm + 2); ctx.clip();
      for (const ly of this.layers) this.drawLayer(ctx, ly);
      ctx.restore();
      // hover crosshair
      if (this.hoverX != null) {
        ctx.strokeStyle = ink3; ctx.setLineDash([3, 3]);
        ctx.beginPath(); ctx.moveTo(this.hoverX + 0.5, this.T); ctx.lineTo(this.hoverX + 0.5, h - this.Bm); ctx.stroke();
        ctx.setLineDash([]);
      }
    }
    drawLayer(ctx, ly) {
      const color = ly.color || MTV.css("--ink");
      if (ly.type === "bars") {
        // ly.x0, ly.dx, ly.values
        const n = ly.values.length;
        const gap = Math.abs(this.sx(ly.x0 + ly.dx) - this.sx(ly.x0)) > 5 ? 1 : 0;
        ctx.fillStyle = color;
        for (let i = 0; i < n; i++) {
          const xa = this.sx(ly.x0 + i * ly.dx), xb = this.sx(ly.x0 + (i + 1) * ly.dx);
          const y = this.sy(ly.values[i]), y0 = this.sy(ly.base || 0);
          ctx.fillRect(Math.min(xa, xb) + gap, Math.min(y, y0), Math.abs(xb - xa) - 2 * gap, Math.abs(y0 - y));
          if (ly.tip) this.hits.push({ x: (xa + xb) / 2, x0: xa, x1: xb, y, text: ly.tip(i) });
        }
      } else if (ly.type === "line") {
        ctx.strokeStyle = color; ctx.lineWidth = ly.width || 2; ctx.lineJoin = "round";
        if (ly.dash) ctx.setLineDash(ly.dash);
        ctx.beginPath();
        let first = true;
        for (let i = 0; i < ly.x.length; i++) {
          const X = this.sx(ly.x[i]), Y = this.sy(ly.y[i]);
          if (!isFinite(Y)) { first = true; continue; }
          if (first) ctx.moveTo(X, Y); else ctx.lineTo(X, Y);
          first = false;
          if (ly.tip) this.hits.push({ x: X, y: Y, text: ly.tip(i) });
        }
        ctx.stroke(); ctx.setLineDash([]);
      } else if (ly.type === "dots") {
        ctx.fillStyle = color;
        const r = ly.r || 1.2;
        for (let i = 0; i < ly.x.length; i++) {
          const X = this.sx(ly.x[i]), Y = this.sy(ly.y[i]);
          ctx.fillRect(X - r, Y - r, 2 * r, 2 * r);
        }
      } else if (ly.type === "band") {
        ctx.fillStyle = color;
        ctx.globalAlpha = ly.alpha || 0.15;
        const ya = this.sy(ly.y1), yb = this.sy(ly.y0);
        ctx.fillRect(this.L, ya, this.W, yb - ya);
        ctx.globalAlpha = 1;
      } else if (ly.type === "hline") {
        ctx.strokeStyle = color; ctx.lineWidth = 1; ctx.setLineDash(ly.dash || [4, 4]);
        const y = Math.round(this.sy(ly.y)) + 0.5;
        ctx.beginPath(); ctx.moveTo(this.L, y); ctx.lineTo(this.W - this.R, y); ctx.stroke(); ctx.setLineDash([]);
        if (ly.label) { ctx.fillStyle = color; ctx.font = "12px " + MTV.css("--sans"); ctx.textAlign = "right"; ctx.textBaseline = "bottom"; ctx.fillText(ly.label, this.W - this.R - 4, y - 3); }
      } else if (ly.type === "stems") {
        ctx.strokeStyle = color; ctx.lineWidth = ly.width || 2;
        for (let i = 0; i < ly.x.length; i++) {
          const X = this.sx(ly.x[i]);
          ctx.beginPath(); ctx.moveTo(X, this.sy(0)); ctx.lineTo(X, this.sy(ly.y[i])); ctx.stroke();
          if (ly.tip) this.hits.push({ x: X, y: this.sy(ly.y[i]), text: ly.tip(i) });
        }
      }
    }
    hover(e) {
      const r = this.cv.getBoundingClientRect();
      const mx = e.clientX - r.left;
      if (!this.hits.length) return;
      let best = null, bd = 1e9;
      for (const hh of this.hits) {
        const d = hh.x0 !== undefined && mx >= hh.x0 && mx <= hh.x1 ? 0 : Math.abs(hh.x - mx);
        if (d < bd) { bd = d; best = hh; }
      }
      if (!best || bd > 30) { this.tip.style.display = "none"; return; }
      this.tip.innerHTML = best.text;
      this.tip.style.display = "block";
      this.tip.style.left = best.x + "px";
      this.tip.style.top = Math.max(best.y, 20) + "px";
      this.hoverX = best.x;
      this.render();
    }
  }
  MTV.Plot = Plot;
  MTV.tickFmt = function (v) {
    const a = Math.abs(v);
    if (a >= 1e6) return v.toExponential(0).replace("e+", "e");
    if (a >= 1000) return (v / 1000).toFixed(a % 1000 === 0 ? 0 : 1) + "k";
    if (a === 0) return "0";
    if (a < 0.01) return v.toExponential(0);
    return String(+v.toFixed(3));
  };

  // schedule a cooperative job: fn() returns true when finished
  MTV.runJob = function (fn, onDone, budgetMs) {
    let cancelled = false;
    function tick() {
      if (cancelled) return;
      const t0 = performance.now();
      let done = false;
      while (!done && performance.now() - t0 < (budgetMs || 12)) done = fn();
      if (done) onDone && onDone();
      else requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
    return () => (cancelled = true);
  };
})();

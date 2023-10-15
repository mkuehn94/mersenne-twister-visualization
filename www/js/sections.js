/* sections.js — all the interactive panels. Each function builds one section. */
(function () {
  "use strict";
  const MTV = window.MTV;
  const { $, $$, h } = MTV;
  let X;

  const MERSENNE_EXP = [2, 3, 5, 7, 13, 17, 19, 31, 61, 89, 107, 127, 521, 607, 1279, 2203, 2281, 3217, 4253, 4423, 9689, 9941, 11213, 19937, 21701, 23209, 44497, 86243, 110503, 132049, 216091];
  const V = { MT19937: 0, PYTHON: 1, KNUTH: 2, GLIB20: 3, PHP_COKUS: 4, PHP_BROKEN: 5, PHP71: 6, PPSSPP: 7, MYBB: 8, ECJ: 9, MT64: 10, STIM: 11, APERY: 12, MT11213B: 13, TT800: 14, GNUGO: 15, R: 16, GZ: 17 };
  const S = { MT: 0, TINYMT: 20, TINYMT_ZERO: 21, SFMT: 22, RANDU: 30, MINSTD: 31, LCG32: 32, XORSHIFT: 33 };
  const TINY = { mat1: 0x8f7011ee, mat2: 0xfc78ff1f, tmat: 0x3793fdff };

  // lazy-init: only build heavy sections when they scroll into view
  function whenVisible(el, fn) {
    let done = false;
    const io = new IntersectionObserver((es) => {
      if (!done && es.some((e) => e.isIntersecting)) { done = true; io.disconnect(); setTimeout(fn, 0); }
    }, { rootMargin: "300px" });
    io.observe(el);
  }
  const timeit = (fn) => { const t = performance.now(); const r = fn(); return [r, performance.now() - t]; };
  const ms = (t) => (t < 10 ? t.toFixed(1) : Math.round(t)) + " ms";

  // ======================================================================== hero
  function hero() {
    const c = MTV.variant(V.MT19937);
    const ds = $("#datasheet");
    const H = (v) => MTV.hex(v, 32);
    const rows = [
      ["w", c.w, "Word size. i.e. standard 32 bit DWORD"],
      ["n", c.n, "Size of the state array: 624, each of size w"],
      ["m", c.m, "offset for the second state word in the twist (x[k+m])"],
      ["r", c.r, "Where the bit(s) are taken and reattached ()"],
      ["a", H(c.a), "twist constant, gets XORed in when the shifted number is odd"],
      ["u, d", c.u + ", " + H(c.d), "Tempering step 1: right shift by 11, XOR back (d is a bitmask, that control which bytes survive. 0xffffffff means all)"],
      ["s, b", c.s + ", " + H(c.b), "Tempering step 2: left shift by 7, keep bits when b has a 1 and XOR back in"],
      ["t, c", c.t + ", " + H(c.c), "Tempering step 3: basically the same operation as above with different parameters"],
      ["l", c.l, "Tempering step 4: right shift by 18 and XOR."],
      ["f", String(c.f), "Seeding parameter that alter the 624 state values at initialization"],
    ];
    const tb = h("table", { class: "tbl" });
    tb.innerHTML = "<tr><th>name</th><th>value</th><th>what it controls</th></tr>";
    for (const [k, v, d] of rows) {
      const kc = h("td"); MTV.tex(kc, k);
      tb.append(h("tr", null, kc, h("td", { class: "m" }, String(v)), h("td", null, d)));
    }
    ds.append(tb);
    MTV.setCfg(c);
    const src = X.src_new(S.MT, 0, 0);
    X.src_fill_u32(src, 12);
    const outs = Array.from(MTV.out32());
    X.obj_free(src);
    const t = $("#ticker");
    t.innerHTML = "mt19937 with seed 0 → " + outs.map((v) => "0x" + v.toString(16).padStart(8, "0")).join(", ") + ", …";
  }

  // ======================================================================== debugger
  function debuggerSection() {
    const E = (label, prog, cfg, seed) => ({ label, prog, cfg, seed });
    const entries = [
      Object.assign(E("MT19937", "mt", MTV.variant(V.MT19937), 5489), { autoSeed: true }),
      E("MT19937-64", "mt", MTV.variant(V.MT64), 5489),
      E("MT11213B", "mt", MTV.variant(V.MT11213B), 5489),
      E("TT800 (twisted GFSR, r = 0)", "mt", MTV.variant(V.TT800), 5489),
      E("TinyMT32", "tiny", TINY, 1),
      E("SFMT19937", "sfmt", {}, 1234),
      E("SFMT607 (5 blocks)", "sfmt", MTV.sfmtParams(0), 1234),
      E("TinyMT64", "tiny64", { mat1: 0xfa051f40, mat2: 0xffd0fff4, tmat: 0x58d02ffeffbfffbcn }, 1),
      E("MELG607-64", "melg", MTV.melgParams(0), 5489),
      E("MELG19937-64", "melg", MTV.melgParams(5), 5489),
      E("XSadd", "xsadd", {}, 1),
    ];
    MTV.dbg = new MTV.Debugger($("#dbg-main"), { entries, bulk: 624, selector: true });
  }
  function openInDebugger(entry) {
    MTV.dbg.addEntry(entry, true);
    $("#debugger").scrollIntoView();
  }

  // ======================================================================== twist / matrices
  function wordMatrices(c) {
    MTV.setCfg(c);
    X.la_word_matrices();
    const a = Array.from(MTV.out32());
    const w = c.w;
    const get = (off) => (i) => MTV.j64(a[off + 2 * i], a[off + 2 * i + 1]);
    return { A: get(0), T: get(2 * w), Ti: get(4 * w) };
  }
  function drawWordMatrix(cv, img, w, color) {
    // row r ↔ input bit w−1−r, column q ↔ output bit w−1−q
    const one = MTV.css(color || "--ink"), zero = MTV.css("--bit0");
    MTV.drawBitMatrix(cv, w, w, (r, q) => (img(w - 1 - r) >> BigInt(w - 1 - q)) & 1n, one, zero, 360);
  }

  // brute-force search for a valid twist constant (only feasible for small p)
  // TODO: blocks the main thread; should use a worker for p > 89
  function findA(c, seed) {
    MTV.setCfg(c);
    const j = X.job_search_new(seed || 1);
    for (let it = 0; it < 4000; it++) {
      X.job_search_step(j, 4096);
      const o = MTV.out32();
      if (o[9] > 0) { const a = MTV.j64(o[10], o[11]); X.obj_free(j); return a; }
    }
    X.obj_free(j);
    return null;
  }
  function charPoly(c, seed) {
    MTV.setCfg(c);
    const L = X.la_charpoly(seed || 5489);
    return { L, terms: Array.from(MTV.out32()) };
  }
  function polyTex(terms, max) {
    const parts = terms.map((e) => (e === 0 ? "1" : e === 1 ? "t" : "t^{" + e + "}"));
    if (max && parts.length > max) return parts.slice(0, max - 3).join(" + ") + " + \\cdots + " + parts.slice(-3).join(" + ");
    return parts.join(" + ");
  }

  // render B as a p×p bit grid; faint lines mark word boundaries
  function drawB(cv, c, maxPx) {
    MTV.setCfg(c);
    const p = X.la_transition(700);
    if (!p) return 0;
    const rows = Array.from(MTV.out32());
    const w32 = Math.ceil(p / 32);
    MTV.drawBitMatrix(cv, p, p, (r, q) => (rows[r * w32 + (q >> 5)] >>> (q & 31)) & 1, MTV.css("--ink"), MTV.css("--bit0"), maxPx || 520);
    const ctx = cv.getContext("2d");
    const sc = cv.width / p;
    ctx.strokeStyle = MTV.css("--read");
    ctx.globalAlpha = 0.35;
    ctx.lineWidth = 1;
    for (let pos = c.w - c.r; pos < p; pos += c.w) {
      const x = Math.round(pos * sc) + 0.5;
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, cv.height); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(0, x); ctx.lineTo(cv.width, x); ctx.stroke();
    }
    ctx.globalAlpha = 1;
    return p;
  }

  let TOY = null;
  function toyConfig() {
    const base = MTV.variant(V.MT19937);
    return Object.assign(base, { w: 8, n: 16, m: 7, r: 1, u: 3, d: 0xffn, s: 2, b: 0xb8n, t: 4, c: 0xf0n, l: 5, umask: 0xfen, lmask: 1n, outAnd: 0xffn, f: 1812433253n & 0xffn });
  }
  function twistSection() {
    const c = MTV.variant(V.MT19937);
    const draw = () => {
      const m = wordMatrices(c);
      drawWordMatrix($("#mat-A"), m.A, 32, "--ink");
      drawWordMatrix($("#mat-T"), m.T, 32, "--ink");
      drawWordMatrix($("#mat-Ti"), m.Ti, 32, "--write");
    };
    draw();
    MTV.onTheme(draw);
    whenVisible($("#mat-B"), () => {
      TOY = toyConfig();
      TOY.a = findA(TOY, 3) || 0xb5n;
      const dB = () => drawB($("#mat-B"), TOY, 520);
      dB();
      MTV.onTheme(dB);
      const cp = charPoly(TOY, 1);
      $("#toyB-title").textContent = "B for w = 8, n = 16, m = 7, r = 1, a = " + MTV.hex(TOY.a, 8) + "  (p = 127)";
      const el = $("#toyB-poly");
      el.innerHTML = "";
      const pe = h("p");
      MTV.tex(pe, "\\chi_B(t) = " + polyTex(cp.terms));
      el.append(pe, h("p", { class: "note" }, cp.terms.length + " nonzero terms. It's irreducible, so every nonzero state has period 2¹²⁷ − 1. (there is a search functions to find those below)"));
    });
  }

  // ======================================================================== period
  function periodSection() {
    const c = MTV.variant(V.MT19937);
    let haveBM = false;
    $("#bm-run").addEventListener("click", () => {
      const [cp, t] = timeit(() => charPoly(c, 5489));
      const out = $("#bm-out");
      out.innerHTML = "";
      const kv = h("div", { class: "kv" });
      kv.innerHTML = "<span>L = <b>" + cp.L + "</b></span><span>nonzero terms <b>" + cp.terms.length + "</b></span><span>time <b>" + ms(t) + "</b></span><span>gap between top terms <b>" + (cp.terms[0] - cp.terms[1]) + "</b></span>";
      out.append(kv);
      const host = $("#bm-terms");
      host.innerHTML = "";
      const pl = h("div");
      host.append(pl);
      const plot = new MTV.Plot(pl, { height: 90, xr: [0, 20000], yr: [0, 1], yticks: [], pad: [8, 10, 26, 10], xlabel: "exponent" });
      plot.draw([{ type: "stems", x: cp.terms, y: cp.terms.map(() => 1), width: 1.5, color: MTV.css("--read"), tip: (i) => "t<sup>" + cp.terms[i] + "</sup>" }]);
      MTV.onTheme(() => plot.draw([{ type: "stems", x: cp.terms, y: cp.terms.map(() => 1), width: 1.5, color: MTV.css("--read"), tip: (i) => "t<sup>" + cp.terms[i] + "</sup>" }]));
      const pe = h("p", { class: "small", style: "line-height:2" });
      MTV.tex(pe, "\\varphi(t) = " + polyTex(cp.terms));
      host.append(pe);
      haveBM = true;
      $("#irr-run").disabled = false;
      $("#irr-note").textContent = "about 2 s";
    });
    $("#irr-run").addEventListener("click", () => {
      if (!haveBM) return;
      charPoly(c, 5489); // make sure LAST_POLY is MT19937's
      const j = X.job_irr_new(0);
      const bar = $("#irr-bar"), out = $("#irr-out"), btn = $("#irr-run");
      btn.disabled = true;
      const t0 = performance.now();
      let st;
      MTV.runJob(() => {
        const done = X.job_irr_step(j, 300);
        st = Array.from(MTV.out32());
        bar.style.width = (100 * st[0]) / st[1] + "%";
        out.innerHTML = "squaring <b class='mono'>" + MTV.fmtInt(st[0]) + "</b> / " + MTV.fmtInt(st[1]);
        return done;
      }, () => {
        const t = performance.now() - t0;
        X.obj_free(j);
        btn.disabled = false;
        out.innerHTML = "";
        const res = h("div");
        if (st[3]) MTV.tex(res, "t^{2^{19937}} \\equiv t \\pmod{\\varphi} \\;\\;\\textcolor{" + MTV.css("--good") + "}{\\checkmark} \\quad\\Rightarrow\\quad \\varphi \\text{ irreducible} \\;\\Rightarrow\\; \\text{period } 2^{19937}-1");
        else res.textContent = "test failed at squaring " + st[4];
        out.append(res, h("div", { class: "note" }, "19 937 modular squarings in " + ms(t) + ". Trick: φ has a 623-term gap right below t¹⁹⁹³⁷, so each reduction pass clears 623 bits at once."));
      });
    });
    // linear complexity profile
    whenVisible($("#lc-plot"), () => {
      const N = 50000, every = 50;
      MTV.setCfg(c);
      const [L, t] = timeit(() => X.la_profile(5489, 31, N, every));
      const prof = Array.from(MTV.out32());
      const xs = prof.map((_, i) => (i + 1) * every);
      const plot = new MTV.Plot($("#lc-plot"), { height: 270, xr: [0, N], yr: [0, 26000], xlabel: "n (bits of the output, bit 31)", ylabel: "L(n)" });
      const layers = () => [
        { type: "line", x: [0, N], y: [0, N / 2], color: MTV.css("--ink-3"), width: 1, dash: [5, 4] },
        { type: "hline", y: 19937, color: MTV.css("--write"), label: "19937" },
        { type: "line", x: xs, y: prof, color: MTV.css("--read"), width: 2, tip: (i) => "n = " + xs[i] + "<br>L = " + prof[i] },
      ];
      plot.draw(layers());
      MTV.onTheme(() => plot.draw(layers()));
      $("#lc-stat").innerHTML = "<span>final L = <b>" + L + "</b></span><span>kink at n ≈ <b>" + 2 * 19937 + "</b></span><span>dashed: n/2</span><span>" + ms(t) + "</span>";
    });
    whenVisible($("#digits"), () => {
      const [len, t] = timeit(() => X.mersenne_digits(19937));
      const s = new TextDecoder().decode(MTV.outb());
      $("#digits").textContent = s;
      $("#digits-cap").innerHTML = "";
      const cap = h("span");
      MTV.tex(cap, "2^{19937}-1 = " + s.slice(0, 10) + "\\ldots" + s.slice(-10));
      $("#digits-cap").append(cap, document.createTextNode(" · " + len + " digits, from 19 937 doublings in base 10⁹ (" + ms(t) + ")"));
    });
  }

  // ======================================================================== tempering
  function temperingSection() {
    const c = MTV.variant(V.MT19937);
    let y = 0x8badf00dn; // arbitrary, user clicks bits or hits random
    const inEl = $("#tx-in");
    inEl.style.gridTemplateColumns = "repeat(32, minmax(0,1fr))";
    const cells = [];
    for (let b = 31; b >= 0; b--) {
      const i = h("i", { title: "bit " + b });
      i.addEventListener("click", () => { y ^= 1n << BigInt(b); render(); });
      cells.push([b, i]);
      inEl.append(i);
    }
    const rowsEl = $("#tx-rows");
    const M = 0xffffffffn;
    const rowHtml = (lab, v, cls, ref) => {
      let s = '<div class="row ' + cls + '"><span class="lab">' + MTV.texStr(lab) + '</span><div class="cells" style="grid-template-columns:repeat(32,minmax(0,1fr))">';
      for (let b = 31; b >= 0; b--) {
        const one = (v >> BigInt(b)) & 1n;
        const chg = ref !== undefined && ((ref >> BigInt(b)) & 1n) !== one;
        s += '<i class="' + (one ? "one" : "") + (chg ? " chg" : "") + '"></i>';
      }
      return s + '</div><span class="hex">' + MTV.hex(v, 32) + "</span></div>";
    };
    function render() {
      for (const [b, i] of cells) i.classList.toggle("one", !!((y >> BigInt(b)) & 1n));
      const y1 = y ^ ((y >> 11n) & c.d);
      const y2 = y1 ^ ((y1 << 7n) & c.b & M);
      const y3 = y2 ^ ((y2 << 15n) & c.c & M);
      const z = y3 ^ (y3 >> 18n);
      MTV.setCfg(c);
      X.temper_one(MTV.lo(z), 0);
      const back = MTV.j64(MTV.out32()[2], MTV.out32()[3]);
      rowsEl.innerHTML =
        rowHtml("y", y, "s0 rd") + rowHtml("\\oplus(y\\gg11)", y1, "s0", y) + rowHtml("\\oplus(y\\ll7)\\land b", y2, "s0", y1) +
        rowHtml("\\oplus(y\\ll15)\\land c", y3, "s0", y2) + rowHtml("z = \\oplus(y\\gg18)", z, "s3", y3) + rowHtml("T^{-1}(z)", back, "s1");
      const pc = (v) => v.toString(2).split("1").length - 1;
      $("#tx-hex").textContent = "popcount " + pc(y) + " → " + pc(z) + (back === y ? " · T⁻¹(T(y)) = y ✓" : "");
    }
    $("#tx-rand").onclick = () => { y = BigInt(Math.floor(Math.random() * 4294967296)); render(); };
    $("#tx-one").onclick = () => { y = 1n << BigInt(Math.floor(Math.random() * 32)); render(); };
    $("#tx-clear").onclick = () => { y = 0n; render(); };
    render();

    // k-distribution, tempered vs not, on a 607-bit twister
    whenVisible($("#kd-plot"), () => {
      const base = Object.assign(MTV.variant(V.MT19937), { n: 19, m: 9, r: 1, umask: 0xfffffffen, lmask: 1n });
      base.a = findA(base, 11) || 0x9908b0dfn;
      $("#kd-title").textContent = "k(v) for MT607: w = 32, n = 19, m = 9, r = 1, a = " + MTV.hex(base.a, 32) + ", MT19937's tempering";
      const plot = new MTV.Plot($("#kd-plot"), { height: 280, xr: [0.5, 32.5], yr: [0.8, 700], logy: true, xlabel: "v (bits of accuracy)", ylabel: "k(v)", xticks: [1, 4, 8, 12, 16, 20, 24, 28, 32] });
      const run = () => {
        const t0 = performance.now();
        const kd = (temper) => {
          MTV.setCfg(Object.assign({}, base, { temper }));
          const j = X.job_kdist_new();
          while (!X.job_kdist_step(j)) {}
          const k = Array.from(MTV.out32());
          X.obj_free(j);
          return k;
        };
        const kt = kd(true), ku = kd(false);
        const t = performance.now() - t0;
        const vs = kt.map((_, i) => i + 1);
        const bound = vs.map((v) => Math.floor(607 / v));
        const delta = (k) => k.reduce((s, x, i) => s + bound[i] - x, 0);
        const layers = () => [
          { type: "bars", x0: 0.5, dx: 1, values: ku, color: MTV.css("--bit0"), tip: (i) => "v = " + (i + 1) + "<br>untempered k = " + ku[i] + "<br>tempered k = " + kt[i] + "<br>bound = " + bound[i] },
          { type: "line", x: vs, y: kt, color: MTV.css("--read"), width: 2 },
          { type: "dots", x: vs, y: kt, color: MTV.css("--read"), r: 2.5 },
          { type: "line", x: vs, y: bound, color: MTV.css("--ink"), width: 1, dash: [4, 3] },
        ];
        plot.draw(layers());
        MTV.onTheme(() => plot.draw(layers()));
        $("#kd-stat").innerHTML =
          '<span><span style="color:var(--read)">●</span> tempered Δ = <b>' + delta(kt) + "</b></span>" +
          '<span><span style="color:var(--ink-3)">■</span> untempered Δ = <b>' + delta(ku) + "</b></span>" +
          "<span>dashed: ⌊607/v⌋</span><span>k(32) = <b>" + kt[31] + "</b> (bound " + bound[31] + ")</span><span>" + ms(t) + "</span>";
      };
      run();
      $("#kd-run").onclick = run;
    });
  }

  // ======================================================================== build your own
  const PRESETS = [
    { p: 17, w: 6, n: 3, m: 1, r: 1 },
    { p: 19, w: 10, n: 2, m: 1, r: 1 },
    { p: 89, w: 8, n: 12, m: 5, r: 7 },
    { p: 127, w: 8, n: 16, m: 7, r: 1 },
    { p: 521, w: 32, n: 17, m: 8, r: 23 },
    { p: 607, w: 32, n: 19, m: 9, r: 1 },
    { p: 1279, w: 32, n: 40, m: 20, r: 1 },
    { p: 2203, w: 32, n: 69, m: 34, r: 5 },
    { p: 4253, w: 32, n: 133, m: 66, r: 3 },
    { p: 19937, w: 32, n: 624, m: 397, r: 31 },
  ];
  function scaledTempering(w) {
    const sc = (x) => Math.max(1, Math.round((x * w) / 32));
    const top = (mask) => (BigInt(mask) >> BigInt(32 - w)) & MTV.mask(w);
    if (w >= 32) return { u: 11, d: 0xffffffffn, s: 7, b: 0x9d2c5680n, t: 15, c: 0xefc60000n, l: 18 };
    return { u: sc(11), d: MTV.mask(w), s: sc(7), b: top(0x9d2c5680), t: sc(15), c: top(0xefc60000), l: sc(18) };
  }
  function buildSection() {
    const sel = $("#bd-preset");
    PRESETS.forEach((pr, i) => sel.append(h("option", { value: i }, "p = " + pr.p + "   (w " + pr.w + ", n " + pr.n + ", r " + pr.r + ")")));
    sel.value = 3;
    const fields = [["w", "dec"], ["n", "dec"], ["m", "dec"], ["r", "dec"], ["a", "hex"], ["u", "dec"], ["d", "hex"], ["s", "dec"], ["b", "hex"], ["t", "dec"], ["c", "hex"], ["l", "dec"], ["f", "dec"]];
    const HELP = {
      w: "bits per number", n: "numbers in the state", m: "offset of the third number mixed in",
      r: "cut point: low r bits come from the next number", a: "twist constant, XORed in when odd",
      u: "tempering 1: right shift", d: "tempering 1: mask", s: "tempering 2: left shift", b: "tempering 2: mask",
      t: "tempering 3: left shift", c: "tempering 3: mask", l: "tempering 4: right shift", f: "seeding multiplier",
    };
    const inputs = {};
    const grid = $("#bd-params");
    for (const [k, kind] of fields) {
      const lab = h("span");
      MTV.tex(lab, k);
      const inp = h("input", { type: "text", class: "num", "data-k": k });
      inp.addEventListener("input", () => onEdit());
      inputs[k] = inp;
      grid.append(h("label", { title: HELP[k] }, lab, inp, h("small", null, HELP[k])));
    }
    let cfg = null;
    let job = null, cancel = null;
    const status = $("#bd-status"), bar = $("#bd-bar"), funnel = $("#bd-funnel"), found = $("#bd-found");
    let kplot = null;

    function readCfg() {
      const base = MTV.variant(V.MT19937);
      const g = (k) => MTV.parseBig(inputs[k].value);
      const c = Object.assign(base, {
        w: Number(g("w")), n: Number(g("n")), m: Number(g("m")), r: Number(g("r")),
        a: g("a"), u: Number(g("u")), d: g("d"), s: Number(g("s")), b: g("b"), t: Number(g("t")), c: g("c"), l: Number(g("l")), f: g("f"),
      });
      for (const k of ["a", "d", "b", "c", "f"]) if (c[k] === null) return null;
      if (!(c.w >= 2 && c.w <= 64 && c.n >= 2 && c.m >= 1 && c.m < c.n && c.r >= 0 && c.r < c.w)) return null;
      const wm = MTV.mask(c.w);
      c.lmask = c.r === 0 ? 0n : MTV.mask(c.r);
      c.umask = wm & ~c.lmask;
      c.outAnd = wm;
      for (const k of ["a", "d", "b", "c", "f"]) c[k] = BigInt.asUintN(64, c[k]) & wm;
      return c;
    }
    function writeInputs(c) {
      for (const [k, kind] of fields) inputs[k].value = kind === "hex" ? MTV.hex(c[k], c.w) : String(c[k]);
    }
    function onEdit() {
      cfg = readCfg();
      const pEl = $("#bd-p");
      if (!cfg) { pEl.innerHTML = '<span class="fail">invalid parameters</span>'; return; }
      const p = MTV.p(cfg);
      const mers = MERSENNE_EXP.includes(p);
      pEl.innerHTML = "p = nw − r = " + cfg.n + "·" + cfg.w + " − " + cfg.r + " = <b>" + p + "</b> " + (mers ? '<span class="pass">2<sup>p</sup> − 1 is prime</span>' : '<span class="fail">not a Mersenne exponent, so irreducible doesn\'t guarantee the full period</span>');
      $("#bd-search").disabled = p > 4500;
      $("#bd-period").disabled = p > 25;
      $("#bd-kdist").disabled = p > 4500;
      $("#bd-period").title = p > 25 ? "only for p ≤ 25 (it steps through all 2^p − 1 states)" : "";
      clearTimeout(bTimer);
      bTimer = setTimeout(() => {
        const cv = $("#bd-B");
        if (p <= 640) { cv.style.display = ""; drawB(cv, cfg, 420); $("#bd-Bcap").textContent = p + " × " + p + ", blue lines = word boundaries"; }
        else { cv.style.display = "none"; $("#bd-Bcap").textContent = "shown for p ≤ 640"; }
      }, 120);
    }
    let bTimer = 0;
    MTV.onTheme(() => cfg && MTV.p(cfg) <= 640 && drawB($("#bd-B"), cfg, 420));
    function loadPreset(i) {
      const pr = PRESETS[i];
      const base = pr.p === 19937 ? MTV.variant(V.MT19937) : Object.assign(MTV.variant(V.MT19937), pr, scaledTempering(pr.w), { a: (1n << BigInt(pr.w - 1)) | 1n, f: 1812433253n & MTV.mask(pr.w) });
      writeInputs(base);
      onEdit();
      found.innerHTML = "";
      funnel.innerHTML = "";
      if (kplot) { kplot.draw([], {}); }
      $("#bd-kstat").innerHTML = "";
      status.innerHTML = pr.p === 19937 ? "That's MT19937 itself. A search at this size costs ~2 s per surviving candidate, so maybe just hit “Test this a” and verify 0x9908b0df." : "Press “Search for a”.";
      bar.style.width = "0";
      if (cancel) cancel();
    }
    sel.addEventListener("change", () => loadPreset(+sel.value));
    loadPreset(3);

    function showFunnel(o) {
      const [tried, deg, root, small, full, , p] = o;
      const nf = o[9];
      const rows = [
        ["candidates a drawn", tried],
        ["deg φ < p (the state space splits)", deg],
        ["φ(0) = 0 or φ(1) = 0", root],
        ["factor of degree ≤ 12", small],
        ["t<sup>2<sup>p</sup></sup> ≢ t", full],
        ["irreducible → period 2<sup>p</sup> − 1", nf],
      ];
      funnel.innerHTML = "<table class='tbl'>" + rows.map(([k, v], i) => "<tr><td>" + k + "</td><td class='m' style='text-align:right'>" + MTV.fmtInt(v) + "</td><td style='width:40%'><div class='progress' style='margin:4px 0'><div style='width:" + (tried ? (100 * v) / tried : 0) + "%;background:" + (i === 5 ? "var(--good)" : i === 0 ? "var(--ink-3)" : "var(--bad)") + "'></div></div></td></tr>").join("") + "</table>" +
        "<p class='note'>found " + nf + " in " + tried + " tries · 1/p = 1/" + p + "</p>";
    }
    $("#bd-search").onclick = () => {
      if (!cfg) return;
      if (cancel) { cancel(); cancel = null; $("#bd-search").textContent = "Search for a"; return; }
      MTV.setCfg(cfg);
      if (job) X.obj_free(job);
      job = X.job_search_new((Math.random() * 1e9) >>> 0);
      const p = MTV.p(cfg);
      $("#bd-search").textContent = "Stop";
      found.innerHTML = "";
      let shown = 0;
      const t0 = performance.now();
      cancel = MTV.runJob(() => {
        X.job_search_step(job, Math.max(64, Math.floor(400000 / p)));
        const o = Array.from(MTV.out32());
        showFunnel(o);
        const nf = o[9];
        for (; shown < nf; shown++) {
          const a = MTV.j64(o[10 + 2 * shown], o[11 + 2 * shown]);
          if (shown >= 64) continue;
          const b = h("button", { onclick: () => { inputs.a.value = MTV.hex(a, cfg.w); onEdit(); status.textContent = "a = " + MTV.hex(a, cfg.w) + " selected"; } }, MTV.hex(a, cfg.w));
          found.append(b);
          if (shown === 0) { inputs.a.value = MTV.hex(a, cfg.w); }
        }
        status.innerHTML = "testing a = <span class='mono'>" + MTV.hex(MTV.j64(o[7], o[8]), cfg.w) + "</span>" + (o[5] ? " · squaring " + o[5] + " / " + p : "");
        const exh = X.job_search_exhausted(job);
        const total = 2 ** (cfg.w - 1);
        bar.style.width = (cfg.w <= 12 ? (100 * o[0]) / total : Math.min(100, (100 * nf) / 6)) + "%";
        if (exh) status.innerHTML = "All " + total + " vectors a with a<sub>w−1</sub> = 1 tried: <b>" + nf + "</b> give an irreducible φ.";
        return exh || (cfg.w > 12 && nf >= 6) || performance.now() - t0 > 60000;
      }, () => {
        cancel = null;
        $("#bd-search").textContent = "Search for a";
        cfg = readCfg();
        if (!X.job_search_exhausted(job)) status.innerHTML = "Done! Click any found value to use it (the first one is already filled in).";
      }, 14);
    };
    $("#bd-test").onclick = () => {
      if (!cfg) return;
      const p = MTV.p(cfg);
      const cp = charPoly(cfg, 5489);
      if (cp.L < p) { status.innerHTML = '<span class="fail">deg φ = ' + cp.L + " < p.</span> The minimal polynomial is too small, so the period is at most 2<sup>" + cp.L + "</sup> − 1."; return; }
      const j = X.job_irr_new(12);
      const t0 = performance.now();
      MTV.runJob(() => {
        const d = X.job_irr_step(j, 200);
        const o = MTV.out32();
        bar.style.width = (100 * o[0]) / o[1] + "%";
        status.textContent = "squaring " + o[0] + " / " + o[1];
        return d;
      }, () => {
        const o = Array.from(MTV.out32());
        X.obj_free(j);
        const mers = MERSENNE_EXP.includes(p);
        status.innerHTML = o[3]
          ? '<span class="pass">φ is irreducible</span> (' + cp.terms.length + " terms, " + ms(performance.now() - t0) + ")" + (mers ? ". 2<sup>p</sup> − 1 is prime, so the period is 2<sup>" + p + "</sup> − 1." : ". 2<sup>p</sup> − 1 is not prime, so the period could still be a proper divisor.")
          : '<span class="fail">φ is reducible</span>' + (o[4] <= 12 ? " (it has a factor of degree ≤ " + o[4] + ")" : "") + ".";
      });
    };
    $("#bd-period").onclick = () => {
      if (!cfg) return;
      const p = MTV.p(cfg);
      if (p > 25) return;
      MTV.setCfg(cfg);
      const lim = 2 ** p + 2;
      const j = X.job_period_new(lim >>> 0, Math.floor(lim / 4294967296));
      const t0 = performance.now();
      MTV.runJob(() => {
        const d = X.job_period_step(j, 200000);
        const o = MTV.out32();
        const stp = o[0] + o[1] * 4294967296;
        bar.style.width = Math.min(100, (100 * stp) / (2 ** p)) + "%";
        status.textContent = "stepping the state: " + MTV.fmtInt(stp);
        return d;
      }, () => {
        const o = Array.from(MTV.out32());
        X.obj_free(j);
        const stp = o[0] + o[1] * 4294967296;
        const full = 2 ** p - 1;
        status.innerHTML = o[3]
          ? "The state returned after <b class='mono'>" + MTV.fmtInt(stp) + "</b> steps " + (stp === full ? '<span class="pass">= 2<sup>' + p + "</sup> − 1, the full period</span>" : '<span class="fail">≠ 2<sup>' + p + "</sup> − 1 = " + MTV.fmtInt(full) + "</span>") + " (" + ms(performance.now() - t0) + ")"
          : "no return within " + MTV.fmtInt(stp) + " steps";
      });
    };
    $("#bd-kdist").onclick = () => {
      if (!cfg) return;
      const p = MTV.p(cfg);
      const kd = (temper) => {
        MTV.setCfg(Object.assign({}, cfg, { temper }));
        const j = X.job_kdist_new();
        while (!X.job_kdist_step(j)) {}
        const k = Array.from(MTV.out32());
        X.obj_free(j);
        return k;
      };
      const [res, t] = timeit(() => [kd(true), kd(false)]);
      const [kt, ku] = res;
      const vs = kt.map((_, i) => i + 1);
      const bound = vs.map((v) => Math.floor(p / v));
      if (!kplot) kplot = new MTV.Plot($("#bd-kplot"), { height: 230 });
      const layers = () => [
        { type: "bars", x0: 0.5, dx: 1, values: ku, color: MTV.css("--bit0"), tip: (i) => "v = " + (i + 1) + "<br>tempered " + kt[i] + "<br>untempered " + ku[i] + "<br>bound " + bound[i] },
        { type: "line", x: vs, y: kt, color: MTV.css("--read"), width: 2 },
        { type: "dots", x: vs, y: kt, color: MTV.css("--read"), r: 2.5 },
        { type: "line", x: vs, y: bound, color: MTV.css("--ink"), width: 1, dash: [4, 3] },
      ];
      kplot.draw(layers(), { xr: [0.5, cfg.w + 0.5], yr: [0.8, p * 1.15], logy: true, xlabel: "v", ylabel: "k(v)", xticks: null });
      MTV.onTheme(() => kplot.draw(layers()));
      const delta = (k) => k.reduce((s, x, i) => s + bound[i] - x, 0);
      $("#bd-kstat").innerHTML = "<span>tempered Δ = <b>" + delta(kt) + "</b></span><span>untempered Δ = <b>" + delta(ku) + "</b></span><span>k(w) = <b>" + kt[cfg.w - 1] + "</b> / " + bound[cfg.w - 1] + "</span><span>" + ms(t) + "</span>";
    };
    $("#bd-debug").onclick = () => {
      if (!cfg) return;
      const p = MTV.p(cfg);
      openInDebugger({ label: "Your MT (p = " + p + ", a = " + MTV.hex(cfg.a, cfg.w) + ")", prog: "mt", cfg: Object.assign({}, cfg), seed: 5489 });
      MTV.addStatsSource && MTV.addStatsSource({ label: "Your MT (p = " + p + ")", kind: S.MT, cfg: Object.assign({}, cfg) });
    };
  }

  // ======================================================================== distributions
  function statsSection() {
    const sources = [
      { label: "MT19937", kind: S.MT, cfg: MTV.variant(V.MT19937) },
      { label: "MT19937-64", kind: S.MT, cfg: MTV.variant(V.MT64) },
      { label: "SFMT19937", kind: S.SFMT },
      { label: "TinyMT32", kind: S.TINYMT },
      { label: "TT800", kind: S.MT, cfg: MTV.variant(V.TT800) },
      { label: "PHP MT_RAND_PHP (broken twist)", kind: S.MT, cfg: MTV.variant(V.PHP_BROKEN) },
      { label: "PPSSPP sceMt19937 (A never applied)", kind: S.MT, cfg: MTV.variant(V.PPSSPP) },
      { label: "MT19937 without tempering", kind: S.MT, cfg: Object.assign(MTV.variant(V.MT19937), { temper: false }) },
      { label: "rusEFI TinyMT32 (all parameters 0)", kind: S.TINYMT_ZERO },
      { label: "RANDU", kind: S.RANDU },
      { label: "MINSTD (Park–Miller)", kind: S.MINSTD },
      { label: "LCG mod 2³² (Numerical Recipes)", kind: S.LCG32 },
      { label: "xorshift32", kind: S.XORSHIFT },
    ];
    const sel = $("#st-src");
    const fill = () => { const v = sel.selectedIndex; sel.innerHTML = ""; sources.forEach((s) => sel.append(h("option", null, s.label))); sel.selectedIndex = Math.max(0, v); };
    fill();
    MTV.addStatsSource = (s, select) => {
      const i = sources.findIndex((x) => x.label === s.label);
      if (i >= 0) sources[i] = s; else sources.push(s);
      fill();
      if (select) { sel.selectedIndex = sources.indexOf(s); run(); }
    };
    MTV.selectStatsSource = (label) => {
      const i = sources.findIndex((x) => x.label === label);
      if (i >= 0) { sel.selectedIndex = i; run(); }
    };

    const plots = {};
    const P = (id, opt) => (plots[id] = plots[id] || new MTV.Plot($(id), opt));
    let kSlider = $("#st-k");

    function newSrc() {
      const s = sources[sel.selectedIndex];
      const seed = MTV.parseBig($("#st-seed").value) || 1n;
      if (s.cfg) MTV.setCfg(s.cfg);
      return X.src_new(s.kind, MTV.lo(seed), MTV.hi(seed));
    }
    const pfmt = (p) => (p < 1e-4 ? p.toExponential(1) : p.toFixed(4));
    const verdict = (p) => (p < 0.001 ? '<span class="fail">p = ' + pfmt(p) + "</span>" : "p = " + pfmt(p));
    let last = {};

    function run() {
      const N = +$("#st-n").value;
      last.N = N;
      // uniform
      {
        const src = newSrc();
        X.stat_uniform(src, N, 50);
        const counts = Array.from(MTV.out32());
        const [chi2, , p] = MTV.outf();
        X.obj_free(src);
        const E = N / 50, sd = Math.sqrt(E * (1 - 1 / 50));
        const mx = Math.max(...counts, E + 3 * sd);
        last.u = () => [
          { type: "band", y0: E - 2 * sd, y1: E + 2 * sd, color: MTV.css("--read"), alpha: 0.12 },
          { type: "bars", x0: 0, dx: 0.02, values: counts, color: MTV.css("--ink"), tip: (i) => "[" + (i * 0.02).toFixed(2) + ", " + ((i + 1) * 0.02).toFixed(2) + ")<br>" + counts[i] + " (E = " + E.toFixed(0) + ")" },
          { type: "hline", y: E, color: MTV.css("--read"), label: "E" },
        ];
        P("#st-u", { height: 220 }).draw(last.u(), { xr: [0, 1], yr: [0, mx * 1.08], xlabel: "u", ylabel: "count" });
        $("#st-u-stat").innerHTML = "χ² = " + chi2.toFixed(1) + " · " + verdict(p);
      }
      // normal
      {
        const src = newSrc();
        X.stat_normal(src, N, 80, -4, 4);
        const counts = Array.from(MTV.out32());
        const [chi2, , p] = MTV.outf();
        X.obj_free(src);
        const wdt = 0.1;
        const dens = counts.map((c) => c / (N * wdt));
        const xs = [], ys = [];
        for (let x = -4; x <= 4.0001; x += 0.05) { xs.push(x); ys.push(X.phi_pdf(x)); }
        last.n = () => [
          { type: "bars", x0: -4, dx: wdt, values: dens, color: MTV.css("--ink"), tip: (i) => "[" + (-4 + i * wdt).toFixed(1) + ", " + (-4 + (i + 1) * wdt).toFixed(1) + ")<br>density " + dens[i].toFixed(4) + "<br>φ = " + X.phi_pdf(-4 + (i + 0.5) * wdt).toFixed(4) },
          { type: "line", x: xs, y: ys, color: MTV.css("--write"), width: 2 },
        ];
        P("#st-norm", { height: 220 }).draw(last.n(), { xr: [-4, 4], yr: [0, 0.45], xlabel: "z", ylabel: "density" });
        $("#st-n-stat").innerHTML = "χ² = " + chi2.toFixed(1) + " · " + verdict(p);
      }
      runClt();
      // 3-D lag
      {
        const src = newSrc();
        X.src_fill_unit(src, 3 * 9000);
        tri = Float64Array.from(MTV.outf());
        X.obj_free(src);
        draw3d();
      }
      // raster + bias
      {
        let src = newSrc();
        X.src_fill_u32(src, 160);
        raster = Uint32Array.from(MTV.out32());
        X.obj_free(src);
        drawRaster();
        src = newSrc();
        X.stat_bitbias(src, N, 1);
        const fr = Array.from(MTV.outf());
        X.obj_free(src);
        const band = 1.96 * Math.sqrt(0.25 / N);
        last.b = () => [
          { type: "band", y0: 0.5 - band, y1: 0.5 + band, color: MTV.css("--read"), alpha: 0.18 },
          { type: "bars", x0: 0.5, dx: 1, values: fr, base: 0.5, color: MTV.css("--ink"), tip: (i) => "bit " + (31 - i) + "<br>ones: " + fr[i].toFixed(4) },
        ];
        const lo = Math.min(0.45, ...fr), hi = Math.max(0.55, ...fr);
        P("#st-bias", { height: 150 }).draw(last.b(), { xr: [0.5, 32.5], yr: [lo - 0.01, hi + 0.01], xticks: [1, 8, 16, 24, 32], xfmt: (v) => "bit " + (32 - v), ylabel: "fraction of 1s" });
      }
      // autocorrelation
      {
        const src = newSrc();
        X.stat_autocorr(src, N, 50);
        const r = Array.from(MTV.outf());
        X.obj_free(src);
        const band = 1.96 / Math.sqrt(N);
        const hs = r.map((_, i) => i + 1);
        const m = Math.max(band * 2.5, ...r.map(Math.abs)) * 1.1;
        last.a = () => [
          { type: "band", y0: -band, y1: band, color: MTV.css("--read"), alpha: 0.15 },
          { type: "stems", x: hs, y: r, color: MTV.css("--ink"), width: 2, tip: (i) => "h = " + (i + 1) + "<br>r = " + r[i].toFixed(5) },
        ];
        P("#st-ac", { height: 200 }).draw(last.a(), { xr: [0, 51], yr: [-m, m], xlabel: "lag h", ylabel: "r_h" });
      }
      // pi
      {
        const src = newSrc();
        X.stat_pi(src, N, 60);
        const a = Array.from(MTV.outf());
        X.obj_free(src);
        const ns = [], err = [];
        for (let i = 0; i < a.length; i += 2) {
          const est = a[i + 1];
          ns.push(a[i]);
          err.push(Math.max(1e-7, Math.abs(est - Math.PI)));
        }
        const gx = [10, N], gy = gx.map((n) => 1.64 / Math.sqrt(n));
        last.p = () => [
          { type: "line", x: gx, y: gy, color: MTV.css("--write"), width: 1.5, dash: [5, 4] },
          { type: "line", x: ns, y: err, color: MTV.css("--read"), width: 2, tip: (i) => "N = " + MTV.fmtInt(ns[i]) + "<br>estimate " + a[2 * i + 1].toFixed(6) + "<br>|err| = " + err[i].toExponential(2) },
        ];
        P("#st-pi", { height: 220 }).draw(last.p(), { xr: [10, N], yr: [1e-5, 2], logx: true, logy: true, xlabel: "N (points)", ylabel: "|estimate − π|" });
        $("#st-pi-stat").textContent = "estimate = " + a[a.length - 1].toFixed(6);
      }
      // gaps
      {
        const src = newSrc();
        X.src_fill_unit(src, N + 1);
        const u = MTV.outf();
        const bins = 40, cnt = new Array(bins).fill(0);
        for (let i = 0; i < N; i++) cnt[Math.min(bins - 1, Math.floor(Math.abs(u[i + 1] - u[i]) * bins))]++;
        X.obj_free(src);
        let chi2 = 0;
        const dens = cnt.map((c) => (c / N) * bins);
        for (let i = 0; i < bins; i++) {
          const e = N * ((1 - i / bins) ** 2 - (1 - (i + 1) / bins) ** 2);
          chi2 += (cnt[i] - e) ** 2 / e;
        }
        const p = X.chi2_pvalue(chi2, bins - 1);
        last.g = () => [
          { type: "bars", x0: 0, dx: 1 / bins, values: dens, color: MTV.css("--ink"), tip: (i) => "D ∈ [" + (i / bins).toFixed(3) + ", " + ((i + 1) / bins).toFixed(3) + ")<br>density " + dens[i].toFixed(3) },
          { type: "line", x: [0, 1], y: [2, 0], color: MTV.css("--write"), width: 2 },
        ];
        P("#st-gap", { height: 200 }).draw(last.g(), { xr: [0, 1], yr: [0, 2.2], xlabel: "D", ylabel: "density" });
        $("#st-g-stat").innerHTML = "χ² = " + chi2.toFixed(1) + " · " + verdict(p);
      }
    }
    function runClt() {
      const N = Math.min(+$("#st-n").value, 200000);
      const k = +kSlider.value;
      $("#st-kv").textContent = k;
      const src = newSrc();
      const bins = 60;
      X.stat_sum(src, k, N, bins);
      const counts = Array.from(MTV.out32());
      const [chi2, , p] = MTV.outf();
      X.obj_free(src);
      const dens = counts.map((c) => (c / N) * bins);
      const xs = [], ih = [], nm = [];
      const sd = Math.sqrt(1 / (12 * k));
      for (let i = 0; i <= 300; i++) {
        const x = i / 300;
        xs.push(x);
        ih.push(k * X.irwin_hall_pdf(k, k * x));
        nm.push(Math.exp(-((x - 0.5) ** 2) / (2 * sd * sd)) / (sd * Math.sqrt(2 * Math.PI)));
      }
      const top = Math.max(...dens, ...ih) * 1.1;
      last.c = () => [
        { type: "bars", x0: 0, dx: 1 / bins, values: dens, color: MTV.css("--ink"), tip: (i) => "mean ∈ [" + (i / bins).toFixed(3) + ", " + ((i + 1) / bins).toFixed(3) + ")<br>density " + dens[i].toFixed(3) },
        { type: "line", x: xs, y: nm, color: MTV.css("--read"), width: 1.5, dash: [5, 4] },
        { type: "line", x: xs, y: ih, color: MTV.css("--write"), width: 2 },
      ];
      P("#st-clt", { height: 220 }).draw(last.c(), { xr: [0, 1], yr: [0, top], xlabel: "Ū (amber: Irwin–Hall, dashed blue: normal)", ylabel: "density" });
      $("#st-c-stat").innerHTML = "χ² vs Irwin–Hall = " + chi2.toFixed(1) + " · " + verdict(p);
    }
    kSlider.addEventListener("input", runClt);

    // ---- 3-D lag plot
    let tri = null;
    let R;
    const cv3 = $("#st-3d");
    function Mat(a) { this.a = a; }
    Mat.prototype.mul = function (o) {
      const a = this.a, b = o.a, c = new Array(9).fill(0);
      for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) for (let k = 0; k < 3; k++) c[3 * i + j] += a[3 * i + k] * b[3 * k + j];
      return new Mat(c);
    };
    function rotX(t) { const c = Math.cos(t), s = Math.sin(t); return new Mat([1, 0, 0, 0, c, -s, 0, s, c]); }
    function rotY(t) { const c = Math.cos(t), s = Math.sin(t); return new Mat([c, 0, s, 0, 1, 0, -s, 0, c]); }
    R = rotY(0.6).mul(rotX(-0.5));
    function draw3d() {
      if (!tri) return;
      const { ctx, w, h: H } = MTV.fitCanvas(cv3, 0.75);
      ctx.clearRect(0, 0, w, H);
      const sc = Math.min(w, H) * 0.52, cx = w / 2, cy = H / 2;
      const a = R.a;
      const proj = (x, y, z) => [cx + sc * (a[0] * x + a[1] * y + a[2] * z), cy - sc * (a[3] * x + a[4] * y + a[5] * z)];
      // cube edges
      ctx.strokeStyle = MTV.css("--rule"); ctx.lineWidth = 1;
      const V8 = [];
      for (let i = 0; i < 8; i++) V8.push([(i & 1) - 0.5, ((i >> 1) & 1) - 0.5, ((i >> 2) & 1) - 0.5]);
      for (let i = 0; i < 8; i++) for (let j = i + 1; j < 8; j++) {
        const d = (i ^ j);
        if (d === 1 || d === 2 || d === 4) { const p = proj(...V8[i]), q = proj(...V8[j]); ctx.beginPath(); ctx.moveTo(p[0], p[1]); ctx.lineTo(q[0], q[1]); ctx.stroke(); }
      }
      ctx.fillStyle = MTV.css("--ink");
      ctx.globalAlpha = 0.55;
      for (let i = 0; i + 2 < tri.length; i += 3) {
        const p = proj(tri[i] - 0.5, tri[i + 1] - 0.5, tri[i + 2] - 0.5);
        ctx.fillRect(p[0] - 0.8, p[1] - 0.8, 1.6, 1.6);
      }
      ctx.globalAlpha = 1;
      ctx.fillStyle = MTV.css("--ink-3"); ctx.font = "12px " + MTV.css("--serif");
      const lab = (t, x, y, z) => { const p = proj(x, y, z); ctx.fillText(t, p[0], p[1]); };
      lab("uᵢ", 0.62, -0.5, -0.5); lab("uᵢ₊₁", -0.5, 0.62, -0.5); lab("uᵢ₊₂", -0.5, -0.5, 0.62);
    }
    let drag = null, spin = 0;
    cv3.addEventListener("mousedown", (e) => (drag = [e.clientX, e.clientY]));
    window.addEventListener("mouseup", () => (drag = null));
    window.addEventListener("mousemove", (e) => {
      if (!drag) return;
      const dx = e.clientX - drag[0], dy = e.clientY - drag[1];
      drag = [e.clientX, e.clientY];
      R = rotX(dy * 0.01).mul(rotY(dx * 0.01)).mul(R);
      draw3d();
    });
    cv3.addEventListener("touchmove", (e) => {
      const t = e.touches[0];
      if (drag) { R = rotX((t.clientY - drag[1]) * 0.01).mul(rotY((t.clientX - drag[0]) * 0.01)).mul(R); draw3d(); }
      drag = [t.clientX, t.clientY];
      e.preventDefault();
    }, { passive: false });
    cv3.addEventListener("touchend", () => (drag = null));
    $("#st-3d-planes").onclick = () => {
      // look along a direction inside the RANDU planes: normal n = (9, −6, 1)
      const n = [9, -6, 1], nn = Math.hypot(...n);
      const e1 = n.map((x) => x / nn);
      let e2 = [e1[1] * 0 - e1[2] * 1, e1[2] * 0 - e1[0] * 0, e1[0] * 1 - e1[1] * 0]; // e1 × (0,0,1)… adjusted below
      e2 = [-e1[1], e1[0], 0];
      const l2 = Math.hypot(...e2); e2 = e2.map((x) => x / l2);
      const e3 = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
      R = new Mat([...e1, ...e3, ...e2]);
      draw3d();
      const i = sources.findIndex((s) => s.kind === S.RANDU);
      if (sources[sel.selectedIndex].kind !== S.RANDU) { sel.selectedIndex = i; run(); }
    };
    $("#st-3d-spin").onclick = () => {
      if (spin) { cancelAnimationFrame(spin); spin = 0; return; }
      const f = () => { R = rotY(0.01).mul(R); draw3d(); spin = requestAnimationFrame(f); };
      spin = requestAnimationFrame(f);
    };
    MTV.onResize(cv3.parentElement, draw3d);

    // ---- raster
    let raster = null;
    const cvr = $("#st-raster");
    function drawRaster() {
      if (!raster) return;
      const rows = raster.length;
      const { ctx, w } = MTV.fitCanvas(cvr, rows * 2);
      const cw = w / 32;
      ctx.fillStyle = MTV.css("--bit0"); ctx.fillRect(0, 0, w, rows * 2);
      ctx.fillStyle = MTV.css("--ink");
      for (let r = 0; r < rows; r++) for (let b = 0; b < 32; b++) if ((raster[r] >>> (31 - b)) & 1) ctx.fillRect(b * cw, r * 2, cw - 0.5, 2);
    }
    MTV.onResize(cvr.parentElement, drawRaster);
    MTV.onTheme(() => {
      for (const [k, id] of [["u", "#st-u"], ["n", "#st-norm"], ["c", "#st-clt"], ["b", "#st-bias"], ["a", "#st-ac"], ["p", "#st-pi"], ["g", "#st-gap"]]) if (last[k] && plots[id]) plots[id].draw(last[k]());
    });

    $("#st-run").onclick = run;
    sel.addEventListener("change", run);
    $("#st-n").addEventListener("change", run);
    $("#st-seed").addEventListener("change", run);
    whenVisible($("#stats"), run);
  }

  // ======================================================================== weak spots
  function weakSection() {
    const c = MTV.variant(V.MT19937);
    MTV.setCfg(c);
    const src = X.src_new(S.MT, 5489, 0);
    X.src_fill_u32(src, 1);
    const z0 = MTV.out32()[0];
    X.obj_free(src);
    MTV.untDbg = new MTV.Debugger($("#dbg-unt"), {
      entries: [{ label: "untemper", prog: "unt", cfg: c, seed: z0, seedText: MTV.hex(z0, 32) }],
      seedLabel: "z =", outputs: false, watch: true, keys: false, buttons: ["step", "loop", "continue"], speed: 20,
    });

    cloneAttack(c);

    whenVisible($("#zx-plot"), () => {
      const series = [
        { label: "MT19937", cfg: MTV.variant(V.MT19937), twists: 1600, color: "--read" },
        { label: "MT11213B", cfg: MTV.variant(V.MT11213B), twists: 2600, color: "--const" },
        { label: "TT800", cfg: MTV.variant(V.TT800), twists: 4000, color: "--write" },
      ];
      for (const s of series) {
        MTV.setCfg(s.cfg);
        X.zero_excess(s.twists);
        const r = Array.from(MTV.outf());
        s.x = r.map((_, i) => Math.max(1, i * s.cfg.n));
        s.y = r;
        const hit = r.findIndex((v) => v > 0.45);
        s.hit = hit >= 0 ? hit * s.cfg.n : null;
      }
      const plot = new MTV.Plot($("#zx-plot"), { height: 280, xr: [1, 1.2e6], yr: [0, 0.56], logx: true, xlabel: "outputs generated", ylabel: "fraction of 1-bits in the state" });
      const layers = () => [
        { type: "hline", y: 0.5, color: MTV.css("--ink-3"), label: "½" },
        ...series.map((s) => ({ type: "line", x: s.x, y: s.y, color: MTV.css(s.color), width: 2, tip: (i) => s.label + "<br>" + MTV.fmtInt(s.x[i]) + " outputs<br>ρ = " + s.y[i].toFixed(4) })),
      ];
      plot.draw(layers());
      MTV.onTheme(() => plot.draw(layers()));
      $("#zx-stat").innerHTML = series.map((s) => '<span><span style="color:var(' + s.color + ')">━</span> ' + s.label + ": ρ > 0.45 after <b>" + (s.hit ? MTV.fmtInt(s.hit) : "—") + "</b> outputs</span>").join("");
    });
  }


  // ======================================================================== cloning attack
  function cloneAttack(c0) {
    const N = 624, COLS = 26, ROWS = 24;
    let c = c0;
    let rw = 0, rwSt = null, rwRaf = 0; // rewind job and its last status
    const M32 = 0xffffffffn;
    let victim = 0, clone = 0, secret = 0, used = 0;
    let seen = [];          // observed outputs z (BigInt)
    let rec = [];           // recovered words x (BigInt)
    let cur = -1;
    let running = false, raf = 0, acc = 0, last = 0;
    let predRows = [], predIndex = 0;
    const speed = $("#atk-speed");
    const rate = () => Math.pow(10, (speed.value / 100) * 3.3); // ~1 … 2000 outputs per second
    const showSpeed = () => ($("#atk-speedlab").textContent = (rate() < 10 ? rate().toFixed(1) : Math.round(rate())) + " outputs/s");
    speed.oninput = showSpeed;
    showSpeed();

    function stepState(i, cls) {
      ["#atk-s1", "#atk-s2", "#atk-s3", "#atk-s4"].forEach((id, k) => {
        $(id).classList.toggle("on", k === i);
        $(id).classList.toggle("done", k < i);
      });
    }

    function newSecret() {
      stop();
      if (victim) X.obj_free(victim);
      if (clone) X.obj_free(clone);
      clone = 0;
      cancelAnimationFrame(rwRaf);
      if (rw) X.obj_free(rw);
      rw = 0; rwSt = null;
      c = MTV.variant(+$("#atk-kind").value);
      secret = (Math.random() * 4294967296) >>> 0;
      used = 1000 + Math.floor(Math.random() * 99000);
      MTV.setCfg(c);
      victim = X.src_new(S.MT, secret, 0);
      for (let left = used; left > 0; left -= 50000) X.src_fill_u32(victim, Math.min(50000, left)); // the part we never saw
      seen = []; rec = []; cur = -1; predRows = []; predIndex = N;
      stepState(0);
      $("#atk-pred-body").innerHTML = "Get all 624 words first. Until then the clone doesn't know the full state.";
      render();
    }

    function untemperSteps(z) {
      // the four inverse steps, last tempering step first (MT19937 constants)
      const inv = (y, k, mask, right) => {
        let x = y;
        const reps = Math.ceil(32 / k) - 1;
        for (let r = 0; r < reps; r++) x = y ^ ((right ? x >> BigInt(k) : (x << BigInt(k)) & M32) & mask);
        return x & M32;
      };
      const s1 = inv(z, c.l, M32, true);
      const s2 = inv(s1, c.t, c.c, false);
      const s3 = inv(s2, c.s, c.b, false);
      const s4 = inv(s3, c.u, c.d, true);
      return [z, s1, s2, s3, s4];
    }

    function observeOne() {
      if (seen.length >= N) return false;
      X.src_fill_u32(victim, 1);
      const z = BigInt(MTV.out32()[0]);
      seen.push(z);
      MTV.setCfg(c);
      X.temper_one(MTV.lo(z), 0);
      const o = MTV.out32();
      const x = MTV.j64(o[2], o[3]); // untempered by the Rust core
      rec.push(x);
      cur = rec.length - 1;
      if (rec.length === N) buildClone();
      return true;
    }

    function buildClone() {
      const p = X.scratch(2 * N);
      const buf = MTV.u32at(p, 2 * N);
      for (let i = 0; i < N; i++) { buf[2 * i] = MTV.lo(rec[i]); buf[2 * i + 1] = 0; }
      MTV.setCfg(c);
      clone = X.src_from_state();
      stop();
      stepState(2);
      predRows = [];
      predIndex = N;
      predictMore(16);
      startRewind();
    }

    // ---- step 4: run the word sequence backwards until it looks freshly seeded
    function startRewind() {
      const p = X.scratch(2 * N);
      const buf = MTV.u32at(p, 2 * N);
      for (let i = 0; i < N; i++) { buf[2 * i] = MTV.lo(rec[i]); buf[2 * i + 1] = 0; }
      MTV.setCfg(c);
      rw = X.job_rewind_new(2000000);
      stepState(3);
      const tick = () => {
        if (!rw) return;
        const k = 1500 + Math.floor((rwSt ? rwSt.steps : 0) / 25);
        const done = X.job_rewind_step(rw, k);
        const o = MTV.out32();
        rwSt = { done, steps: o[1] + o[2] * 4294967296, found: o[3], seed: MTV.j64(o[4], o[5]) };
        renderKnow();
        if (!done) rwRaf = requestAnimationFrame(tick);
        else finishRewind();
      };
      rwRaf = requestAnimationFrame(tick);
    }
    function finishRewind() {
      ["#atk-s4"].forEach((id) => $(id).classList.add("done"));
      $("#atk-s4").classList.remove("on");
      if (rwSt.found) {
        // independent check: a brand-new generator seeded with s, run j steps, must print what we observed
        MTV.setCfg(c);
        const g = X.src_new(S.MT, MTV.lo(rwSt.seed), MTV.hi(rwSt.seed));
        const j = rwSt.steps - N;
        for (let left = j; left > 0; left -= 50000) X.src_fill_u32(g, Math.min(50000, left));
        X.src_fill_u32(g, N);
        const a = MTV.out32();
        let same = 0;
        for (let i = 0; i < N; i++) if (BigInt(a[i]) === seen[i]) same++;
        X.obj_free(g);
        rwSt.verify = same;
      }
      renderKnow();
    }

    // ---- the attacker's notebook: the victim's program with every secret as ? / range / value
    function renderKnow() {
      const n = rec.length;
      const U = "\\textcolor{#d14}{\\mathbf{?}}";
      const RNG = (t) => "\\textcolor{#c27a00}{" + t + "}";
      const OK = (t) => "\\textcolor{#2e8b57}{" + t + "}";
      const hx = (v) => "\\texttt{" + MTV.hex(v, 32) + "}";
      const seedMode = c.seedMode;
      const seedName = { 0: "\\mathrm{init\\_genrand}", 1: "\\mathrm{knuth69069}", 3: "\\mathrm{init\\_by\\_array}" };
      const rs = rwSt;
      const found = rs && rs.found;
      const gaveUp = rs && rs.done && !rs.found;
      const j = found ? rs.steps - N : null;
      const L = [];
      const line = (code, cm) => L.push([code, cm || ""]);
      const ti = (x) => MTV.fmtInt(x).replace(/,/g, "{,}");
      // public part
      line("w, n, m, r = 32, 624, 397, 31;\\;\\; a = " + hx(c.a) + ";\\;\\; (u, s, t, l) = (11, 7, 15, 18);\\; b = " + hx(c.b) + ",\\; c = " + hx(c.c), "public: the algorithm is in every textbook");
      // the seed
      if (found) line("s \\gets " + OK(hx(rs.seed) + " = " + rs.seed), "recovered" + (rs.seed === BigInt(secret) ? " ✓ (that is the secret)" : ""));
      else if (gaveUp) line("s \\in " + RNG("[0, 2^{32})"), "still 4 294 967 296 candidates, see below");
      else line("s \\gets " + U, "s \\in [0, 2^{32}):\\ \\text{4 294 967 296 candidates}");
      // seeding routine
      if (found) line("x_0 \\dots x_{623} \\gets " + OK(seedName[rs.found === 1 ? 0 : 1] + "(s)"), rs.found === 1 ? "x_i = 1812433253\\,(x_{i-1} \\oplus x_{i-1} \\gg 30) + i \\text{ holds for the rewound words}" : "x_i = 69069\\,x_{i-1} \\text{ holds for the rewound words}");
      else if (gaveUp) line("x_0 \\dots x_{623} \\gets " + U + "(s)", "no simple recurrence anywhere in the last 2 000 000 words");
      else line("x_0 \\dots x_{623} \\gets " + U + "(s)", "\\text{which seeding routine? find out by rewinding}");
      // how many numbers were burned before we looked
      if (found) line("\\textbf{repeat } " + OK(ti(j)) + "\\textbf{ times: } \\mathrm{next}()", "j recovered" + (j === used ? " ✓" : ""));
      else if (rs) line("\\textbf{repeat } " + RNG("j \\ge " + ti(Math.max(0, rs.steps - N))) + "\\textbf{ times: } \\mathrm{next}()", gaveUp ? "j \\ge 1\\,999\\,376,\\; \\text{gave up}" : "\\text{rewinding} \\dots\\; " + ti(rs.steps) + "\\text{ words back}");
      else line("\\textbf{repeat } " + U + "\\textbf{ times: } \\mathrm{next}()", "j \\in \\{0, 1, 2, \\dots\\}");
      L.push(null); // separator
      // the window we observed
      const shown = [];
      for (let i = 0; i < 4; i++) shown.push(i < n ? OK(hx(rec[i])) : U);
      const tail = n >= N ? OK(hx(rec[N - 1])) : U;
      line("x_{j+624},\\, x_{j+625}, \\dots, x_{j+1247} = (" + shown.join(",\\, ") + ",\\, \\dots,\\, " + tail + ")", n >= N ? "all 624 words known" : "\\text{" + n + " / 624 known, the rest } \\in [0, 2^{32})");
      if (found) line("j + 624 = " + OK(String(j + 624)), "so the first number we saw was output no. " + MTV.fmtInt(j) + " of the victim");
      // what comes next
      if (clone && predRows.length) {
        const [, p0] = predRows[0];
        line("z_{j+624} \\gets T(x_{j+1248}) = " + OK(hx(p0)), "the next output, before the victim prints it");
      } else line("z_{j+624} \\gets T(x_{j+1248}) = " + U, "needs all 624 words first");
      if (found) $("#atk-victim").innerHTML = "<span>seed <b class='pass'>" + MTV.hex(rs.seed, 32) + "</b> (recovered)</span><span>numbers used before we looked <b class='pass'>" + MTV.fmtInt(j) + "</b> (recovered)</span><span>observed <b>" + n + " / " + N + "</b></span>";
      else if (gaveUp) $("#atk-victim").innerHTML = "<span>seed <b>still hidden</b></span><span>numbers used before we looked <b>≥ " + MTV.fmtInt(rs.steps - N) + "</b></span><span>observed <b>" + n + " / " + N + "</b></span>";
      const box = $("#atk-know");
      let html = "";
      let no = 0;
      for (const r of L) {
        if (!r) { html += "<div class='ln sep'>── we start watching here ──</div>"; continue; }
        html += "<div class='ln'><span class='no'>" + no++ + "</span><span>" + MTV.texStr(r[0]) + "</span><span class='cm'>" + (r[1] ? "▹ " + (/[\\^_{]/.test(r[1]) ? MTV.texStr(r[1]) : r[1]) : "") + "</span></div>";
      }
      if (found && rs.verify !== undefined) html += "<p class='small'><b class='pass'>Seed recovered.</b> A brand-new generator seeded with s = " + rs.seed + " and run for " + MTV.fmtInt(j) + " numbers reproduces " + rs.verify + " / 624 of the outputs we observed. The twist only loses the low 31 bits of x<sub>0</sub>, and the seeding formula never needed them.</p>";
      if (gaveUp) html += "<p class='small'>CPython seeds with <code>init_by_array</code>, which stirs the key through every word twice. The rewound state doesn't satisfy any one-line recurrence, so there's no shortcut back to s. Prediction still works perfectly though (see below), and you could still brute-force all 2<sup>32</sup> keys offline.</p>";
      box.innerHTML = html;
    }

    function predictMore(k) {
      X.src_fill_u32(clone, k);
      const p = Array.from(MTV.out32());
      X.src_fill_u32(victim, k);
      const a = Array.from(MTV.out32());
      for (let i = 0; i < k; i++) predRows.push([predIndex++, p[i], a[i]]);
      renderPred();
    }

    function renderPred(extra) {
      const rows = predRows.slice(-16);
      const ok = predRows.every(([, p, a]) => p === a);
      const body = $("#atk-pred-body");
      body.innerHTML =
        "<p><b class='pass'>State recovered from 624 outputs.</b> The clone got loaded with the 624 recovered words and runs on its own now. It has never seen the numbers below, the real generator makes them separately. And they match.</p>" +
        "<div class='outlog'><table><tr><th>output</th><th>clone predicts</th><th>real generator</th><th></th></tr>" +
        rows.map(([i, p, a]) => "<tr class='" + (p === a ? "match" : "nomatch") + "'><td>z<sub>" + i + "</sub></td><td>" + MTV.hex(p, 32) + "</td><td>" + MTV.hex(a, 32) + "</td><td>" + (p === a ? "✓ same" : "✗") + "</td></tr>").join("") +
        "</table></div>" +
        "<div class='toolbar'><button id='atk-more'>Predict 16 more</button><button id='atk-million'>Check the next 1 000 000</button></div>" +
        "<p id='atk-million-out'>" + (extra || (ok ? "So far " + predRows.length + " of " + predRows.length + " predictions are correct." : "mismatch")) + "</p>" +
        "<p class='note'>(For the record: the secret seed was " + MTV.hex(secret, 32) + " and the victim had burned " + MTV.fmtInt(used) + " numbers before we started watching. Predicting needed neither of those. Rewinding, in the box above, finds both anyway.)</p>";
      $("#atk-more").onclick = () => predictMore(16);
      $("#atk-million").onclick = () => {
        const K = 1000000;
        let same = 0;
        const t0 = performance.now();
        for (let done = 0; done < K; done += 100000) {
          X.src_fill_u32(clone, 100000);
          const p = MTV.out32().slice();
          X.src_fill_u32(victim, 100000);
          const a = MTV.out32();
          for (let i = 0; i < 100000; i++) if (p[i] === a[i]) same++;
        }
        predIndex += K;
        renderPred("Checked the next " + MTV.fmtInt(K) + " outputs: <b>" + MTV.fmtInt(same) + " / " + MTV.fmtInt(K) + "</b> identical (" + ms(performance.now() - t0) + ").");
      };
    }

    // ---- views
    const cv = $("#atk-grid");
    let geom = null;
    function drawGrid() {
      const { ctx, w } = MTV.fitCanvas(cv, (ROWS / COLS) * 1.0);
      const cell = w / COLS;
      geom = { cell };
      const bit0 = MTV.css("--bit0"), ink = MTV.css("--ink"), write = MTV.css("--write");
      ctx.clearRect(0, 0, w, ROWS * cell);
      for (let i = 0; i < N; i++) {
        const x = (i % COLS) * cell, y = Math.floor(i / COLS) * cell;
        ctx.fillStyle = bit0;
        ctx.fillRect(x + 1, y + 1, cell - 2, cell - 2);
        if (i < rec.length) {
          if (i === cur) ctx.fillStyle = write;
          else { ctx.fillStyle = ink; ctx.globalAlpha = 0.25 + 0.65 * (Number(rec[i] >> 24n) / 255); }
          ctx.fillRect(x + 1, y + 1, cell - 2, cell - 2);
          ctx.globalAlpha = 1;
        }
      }
    }
    cv.addEventListener("mousemove", (e) => {
      const r = cv.getBoundingClientRect();
      const i = Math.floor((e.clientY - r.top) / geom.cell) * COLS + Math.floor((e.clientX - r.left) / geom.cell);
      const tip = $("#atk-tip");
      if (i < 0 || i >= N) { tip.style.display = "none"; return; }
      tip.innerHTML = i < rec.length ? "z = " + MTV.hex(seen[i], 32) + "<br>x = T⁻¹(z) = " + MTV.hex(rec[i], 32) : "word " + i + ": not recovered yet";
      tip.style.display = "block";
      tip.style.left = e.clientX - r.left + "px";
      tip.style.top = e.clientY - r.top + "px";
    });
    cv.addEventListener("mouseleave", () => ($("#atk-tip").style.display = "none"));
    cv.addEventListener("click", (e) => {
      const r = cv.getBoundingClientRect();
      const i = Math.floor((e.clientY - r.top) / geom.cell) * COLS + Math.floor((e.clientX - r.left) / geom.cell);
      if (i < 0 || i >= rec.length) return;
      cur = i;
      render();
      MTV.untDbg.seedIn.value = MTV.hex(seen[i], 32);
      MTV.untDbg.reset();
      $("#dbg-unt").scrollIntoView({ block: "center" });
    });
    MTV.onResize(cv.parentElement, drawGrid);

    const rowHtml = (lab, v, cls, ref) => {
      let s = '<div class="row ' + cls + '"><span class="lab">' + MTV.texStr(lab) + '</span><div class="cells" style="grid-template-columns:repeat(32,minmax(0,1fr))">';
      for (let b = 31; b >= 0; b--) {
        const one = (v >> BigInt(b)) & 1n;
        const chg = ref !== undefined && ((ref >> BigInt(b)) & 1n) !== one;
        s += '<i class="' + (one ? "one" : "") + (chg ? " chg" : "") + '"></i>';
      }
      return s + '</div><span class="hex">' + MTV.hex(v, 32) + "</span></div>";
    };

    function render() {
      const n = rec.length;
      $("#atk-victim").innerHTML = "<span>seed <b>hidden</b></span><span>numbers used before we looked <b>unknown</b></span><span>observed so far <b>" + n + " / " + N + "</b></span>";
      $("#atk-seen").innerHTML = n
        ? "<table><tr><th>observed</th><th>z</th></tr>" + seen.slice(-14).map((z, k, arr) => { const i = n - arr.length + k; return "<tr" + (i === n - 1 ? " class='new'" : "") + "><td>z<sub>" + i + "</sub></td><td>" + MTV.hex(z, 32) + "</td></tr>"; }).reverse().join("") + "</table>"
        : "<p class='note'>Nothing seen yet. Hit Run or “Observe + untemper one output”.</p>";
      $("#atk-count").textContent = n + " / " + N + " words";
      $("#atk-bar").style.width = (100 * n) / N + "%";
      const bits = $("#atk-bits");
      if (cur >= 0) {
        const st = untemperSteps(seen[cur]);
        $("#atk-ut-title").innerHTML = "Untempering z<sub>" + cur + "</sub> → x<sub>" + cur + "</sub>";
        bits.innerHTML =
          rowHtml("z_{" + cur + "}\\ \\text{(output)}", st[0], "s0 rd") +
          rowHtml("\\text{undo step 4: } \\gg 18", st[1], "s0", st[0]) +
          rowHtml("\\text{undo step 3: } \\ll 15", st[2], "s0", st[1]) +
          rowHtml("\\text{undo step 2: } \\ll 7", st[3], "s0", st[2]) +
          rowHtml("\\text{undo step 1} \\Rightarrow x_{" + cur + "}", st[4], "s3", st[3]);
        MTV.setCfg(c);
        X.temper_one(MTV.lo(st[4]), 0);
        const back = BigInt(MTV.out32()[0]);
        $("#atk-check").innerHTML = (back === seen[cur] ? "<span class='pass'>check: T(x) = z ✓</span>" : "<span class='fail'>check failed</span>") +
          " · this word is now known exactly; it is word " + cur + " of the recovered state." + (rec[cur] === st[4] ? "" : " <span class='fail'>(JS and Rust disagree)</span>");
      } else {
        $("#atk-ut-title").textContent = "Untempering";
        bits.innerHTML = "<div class='empty'>Every output we catch gets untempered here, bit by bit.</div>";
        $("#atk-check").innerHTML = "";
      }
      if (n > 0 && n < N) stepState(running || n > 0 ? 1 : 0);
      drawGrid();
      renderKnow();
    }

    // ---- running
    function stop() {
      running = false;
      cancelAnimationFrame(raf);
      $("#atk-run").textContent = "Run";
    }
    function tick(now) {
      if (!running) return;
      acc += (rate() * Math.min(100, now - last)) / 1000;
      last = now;
      let k = Math.floor(acc);
      acc -= k;
      while (k-- > 0 && rec.length < N) observeOne();
      if (clone) return render();
      render();
      raf = requestAnimationFrame(tick);
    }
    $("#atk-run").onclick = () => {
      if (running) return stop();
      if (rec.length >= N) newSecret();
      running = true;
      acc = 1; last = performance.now();
      $("#atk-run").textContent = "Pause";
      raf = requestAnimationFrame(tick);
    };
    $("#atk-step").onclick = () => { stop(); if (rec.length >= N) newSecret(); observeOne(); render(); };
    $("#atk-finish").onclick = () => { stop(); while (rec.length < N) observeOne(); render(); };
    $("#atk-new").onclick = newSecret;
    $("#atk-kind").onchange = newSecret;
    newSecret();
  }

  // ======================================================================== variants
  // kind ids of crate/src/kinds.rs
  const K = { TINYMT64: 100, WRK2: 101, MELG0: 110, SFMT0: 120, XSADD: 130, GZ: 140, MINECRAFT: 141, OASIS07: 142, OASIS11: 143, OCTAVE: 144, RUBY: 145, RINDEX: 146, CELLKEY: 147, GLIB_DOUBLE: 148, GLIB_RANGE: 149, GIMP: 150, SDCMICRO: 151, BEAST2_M1: 152, PROFANITY: 153, QISKIT: 154, QUACKLE: 155 };
  const SFMT_EXP = [607, 1279, 2281, 4253, 11213, 19937, 44497, 86243, 132049, 216091];
  const MELG_EXP = [607, 1279, 2281, 4253, 11213, 19937, 44497];
  const SFMT_USERS = { 19937: "Pokémon Gen 7, Intel MKL, TrinityCore, gnubg" };
  const MELG_USERS = { 19937: "64-bit scientific simulation (Harase & Kimoto reference code)" };


  // upstream source code (public), keyed by row name
  const GH = "https://github.com/";
  const UPSTREAM = {
    "MT19937 reference": ["http://www.math.sci.hiroshima-u.ac.jp/m-mat/MT/MT2002/emt19937ar.html", "mt19937ar.c"],
    "CPython random": [GH + "python/cpython/blob/main/Modules/_randommodule.c", "cpython"],
    "1998 seeding": [GH + "NOAA-PSL/stochastic_physics/blob/master/mersenne_twister.F90", "NOAA-PSL"],
    "BEAST 2 Randomizer": [GH + "CompEvol/beast2/blob/master/src/beast/base/util/Randomizer.java", "beast2"],
    "GLib 2.0 GRand": [GH + "GNOME/glib/blob/main/glib/grand.c", "glib"],
    "PHP 3.0.6 – 5.2.0": [GH + "php/php-src/blob/master/ext/random/engine_mt19937.c", "php-src"],
    "gz-math Rand": [GH + "gazebosim/gz-math/blob/main/src/Rand.cc", "gz-math"],
    "Minecraft Bedrock slime chunks": [GH + "depressed-pho/slime-finder-pe", "slime-finder-pe"],
    "OasisLMF gulcalc": [GH + "OasisLMF/ktools/blob/master/src/gulcalc/getrands.h", "ktools"],
    "OasisLMF correlated writer": [GH + "OasisLMF/ktools/blob/master/src/gulcalc/getrands.h", "ktools"],
    "ECJ": [GH + "GMUEClab/ecj/blob/master/ecj/src/main/java/ec/util/MersenneTwisterFast.java", "ecj"],
    "PHP MT_RAND_PHP": [GH + "php/php-src/blob/master/ext/random/engine_mt19937.c", "php-src"],
    "PHP 7.1+": [GH + "php/php-src/blob/master/ext/random/engine_mt19937.c", "php-src"],
    "PPSSPP sceMt19937": [GH + "hrydgard/ppsspp/blob/master/Core/HLE/sceMt19937.cpp", "ppsspp"],
    "MyBB my_rand": [GH + "mybb/mybb", "mybb"],
    "Ruby Random#rand": [GH + "ruby/ruby/blob/master/random.c", "ruby"],
    "Octave randu53": [GH + "gnu-octave/octave/blob/default/liboctave/numeric/randmtzig.cc", "octave"],
    "GLib g_rand_double": [GH + "GNOME/glib/blob/main/glib/grand.c", "glib"],
    "GLib g_rand_int_range": [GH + "GNOME/glib/blob/main/glib/grand.c", "glib"],
    "R sample(), rounding": [GH + "wch/r-source/blob/trunk/src/main/RNG.c", "r-source"],
    "cellKey record keys": [GH + "sdcTools/cellKey/blob/master/R/ck_generate_rkeys.R", "cellKey"],
    "GIMP solid noise": [GH + "GNOME/gimp", "gimp"],
    "sdcMicro record swapping": [GH + "sdcTools/sdcMicro/blob/master/src/recordSwap/recordSwap.cpp", "sdcMicro"],
    "MT19937-64": ["http://www.math.sci.hiroshima-u.ac.jp/m-mat/MT/emt64.html", "mt19937-64.c"],
    "Stim": [GH + "quantumlib/Stim/blob/main/src/stim/util_bot/probability_util.h", "Stim"],
    "Apery": [GH + "HiraokaTakuya/apery/blob/master/src/mt64bit.hpp", "apery"],
    "Qiskit Aer": [GH + "Qiskit/qiskit-aer/blob/main/src/framework/rng.hpp", "qiskit-aer"],
    "Quackle": [GH + "quackle/quackle/blob/master/datamanager.cpp", "quackle"],
    "MT11213B": [GH + "boostorg/random/blob/develop/include/boost/random/mersenne_twister.hpp", "boost.random"],
    "TT800": ["https://www.gnu.org/software/gsl/", "GSL"],
    "GNU Go TT800": ["https://ftp.gnu.org/gnu/gnugo/gnugo-3.8.tar.gz", "gnugo-3.8 (utils/random.c)"],
    "TinyMT32": [GH + "MersenneTwister-Lab/TinyMT", "TinyMT"],
    "rusEFI TinyMT32": [GH + "rusefi/rusefi/blob/master/firmware/controllers/algo/launch_control.cpp", "rusefi"],
    "TinyMT64": [GH + "MersenneTwister-Lab/TinyMT", "TinyMT"],
    "wrk2 TinyMT64": [GH + "giltene/wrk2/blob/master/src/wrk.c", "wrk2"],
    "XSadd": [GH + "mljs/xsadd", "mljs/xsadd"],
  };
  const upstream = (r) => UPSTREAM[r.name] || (r.sfmtIdx !== undefined ? [GH + "MersenneTwister-Lab/SFMT", "SFMT"] : r.melgIdx !== undefined ? [GH + "sharase/melg-64", "melg-64"] : null);

  function variantRows() {
    const R = [];
    const g = (title) => R.push({ group: title });
    g("MT19937, different seeding");
    R.push(
      { name: "MT19937 reference", diff: "x_i = 1812433253(x_{i-1} \\oplus x_{i-1}\\gg30) + i", where: "std::mt19937, NumPy RandomState, MATLAB twister, R's core, Excel 2010+", file: "internal.c · 602 catalog entries", v: V.MT19937, p: 19937 },
      { name: "CPython random", diff: "\\mathrm{init\\_by\\_array}(\\mathrm{key}(s))", where: "random.seed(int), Ruby", file: "kinds/mt19937_seeded.c", v: V.PYTHON, p: 19937 },
      { name: "1998 seeding", diff: "x_i = 69069\\,x_{i-1},\\; 0 \\mapsto 4357", where: "NOAA stochastic physics, BEAST 2", file: "kinds/noaa_ncep_iredell…, kinds/beast2…", v: V.KNUTH, p: 19937 },
      { name: "BEAST 2 Randomizer", diff: "s \\mapsto s - 1\\ (1 \\mapsto 4357),\\; x_i = 69069\\,x_{i-1}", where: "BEAST 2 Randomizer.setSeed", file: "kinds/beast2_mersennetwisterfast_69069seed.c (mode 1)", kind: K.BEAST2_M1, p: 19937 },
      { name: "GLib 2.0 GRand", diff: "x_i = 69069\\,x_{i-1},\\; 0 \\mapsto \\texttt{0x6b842128}", where: "G_RANDOM_VERSION=2.0", file: "kinds/glib_g_rand_mt19937_init_2_0.c", v: V.GLIB20, p: 19937 },
      { name: "PHP 3.0.6 – 5.2.0", diff: "x = s \\lor 1,\\; x_i = 69069^{\\,i} x,\\; z \\gg 1", where: "mt_rand()", file: "kinds/mt19937_php.c", v: V.PHP_COKUS, p: 19937 },
      { name: "gz-math Rand", diff: "x_{0..623} = \\mathrm{seed\\_seq}\\{s\\}.\\mathrm{generate}()", where: "Gazebo sensor noise (gz::math::Rand::Seed)", file: "kinds/gz_math_rand_mt19937_seedseq.c", kind: K.GZ, p: 19937 },
      { name: "Minecraft Bedrock slime chunks", diff: "s = (c_x \\cdot \\texttt{0x1f1f1f1f}) \\oplus c_z,\\; (c_x, c_z) = (3, 5)", where: "slime-chunk test n mod 10 = 0; the seed box is ignored", file: "kinds/minecraft_bedrock_slime_chunk_mt19937.c", kind: K.MINECRAFT, p: 19937 },
      { name: "OasisLMF gulcalc", diff: "s' = (1543270363\\,g + 1943272559\\,e + s) \\bmod 2^{31},\\; (g, e) = (0, 7)", where: "catastrophe-loss Monte Carlo, hashed seed", file: "kinds/oasislmf_ktools_gulcalc_hashedseed_mt19937.c", kind: K.OASIS07, p: 19937 },
      { name: "OasisLMF correlated writer", diff: "s' \\text{ as above},\\; (g, e) = (1, 1)", where: "gulcalc correlated stream", file: "kinds/oasislmf_ktools_gulcalc_hashedseed_mt19937.c", kind: K.OASIS11, p: 19937 },
      { name: "ECJ", diff: "\\text{discard } 2\\cdot624+1 = 1249", where: "ec.Evolve.primeGenerator", file: "kinds/ecj_mersennetwisterfast_primed.c", v: V.ECJ, p: 19937 },
    );
    g("MT19937, changed twist or output");
    R.push(
      { name: "PHP MT_RAND_PHP", diff: "\\text{if } \\textcolor{#d14}{x_k}\\text{ odd}: v \\mathrel{\\oplus}= a", where: "PHP 5.2.1 – 7.0 mt_rand(), HHVM, PHP 8.2 MT_RAND_PHP mode", file: "kinds/mt19937_php.c", v: V.PHP_BROKEN, p: 19937 },
      { name: "PHP 7.1+", diff: "z \\gg 1", where: "mt_rand(), Random\\Engine\\Mt19937", file: "kinds/mt19937_php.c", v: V.PHP71, p: 19937 },
      { name: "PPSSPP sceMt19937", diff: "\\text{lower mask} = \\texttt{0x80000000} \\Rightarrow y \\text{ even} \\Rightarrow A \\text{ never applied}", where: "PSP emulator HLE", file: "kinds/ppsspp_sce_mt19937_broken_twist.c", v: V.PPSSPP, p: 19937 },
      { name: "MyBB my_rand", diff: "(z \\gg 1) \\oplus \\mathit{mask}", where: "MyBB 1.4.12 – 1.8.6", file: "kinds/mt19937_xor_mask.c", v: V.MYBB, p: 19937 },
    );
    g("MT19937, numbers mapped to floats, ranges or permutations");
    R.push(
      { name: "Ruby Random#rand", diff: "\\Big\\lfloor 2^{32}\\,\\frac{(z_1 \\gg 5)\\,2^{26} + (z_2 \\gg 6)}{2^{53}} \\Big\\rfloor", where: "MRI genrand_real", file: "kinds/ruby_mri_int_pair_to_real_exclusive_53bit.c", kind: K.RUBY, p: 19937 },
      { name: "Octave randu53", diff: "\\text{as Ruby, numerator} + 0.4", where: "liboctave randmtzig.c", file: "kinds/octave_randu53_plus_0p4_mapping.c", kind: K.OCTAVE, p: 19937 },
      { name: "GLib g_rand_double", diff: "u = (z_1/2^{32} + z_2)/2^{32},\\; \\text{redraw if } u \\ge 1", where: "GLib grand.c", file: "kinds/glib_g_rand_double_52bit.c", kind: K.GLIB_DOUBLE, p: 19937 },
      { name: "GLib g_rand_int_range", diff: "z \\bmod 10^6,\\; \\text{reject } z > 2^{32} - 1 - (2^{32} \\bmod 10^6)", where: "GLib ≥ 2.2", file: "kinds/glib_g_rand_int_range_2_2_rejection.c", kind: K.GLIB_RANGE, p: 19937 },
      { name: "R sample(), rounding", diff: "\\lfloor 10^6 u \\rfloor,\\; x \\gets 69069x + 1 \\text{ seeding}", where: "R ≤ 3.5 sample() (bias: Ottoboni & Stark 2018)", file: "kinds/r_unif_index_rounding_bias.c", kind: K.RINDEX, p: 19937 },
      { name: "cellKey record keys", diff: "\\lfloor 10^8 u + \\tfrac12 \\rfloor,\\; u \\text{ from R}", where: "Eurostat cell-key perturbation", file: "kinds/cellkey_record_key_r_runif.c", kind: K.CELLKEY, p: 19937 },
      { name: "GIMP solid noise", diff: "32 \\text{ swaps in } \\pi \\in S_{64} \\text{ by } z \\bmod 64,\\; \\text{then emit } \\pi_0, \\pi_1, \\dots", where: "plug-ins/common/noise-solid.c", file: "kinds/gimp_solid_noise_perm_grad_shuffle.c", kind: K.GIMP, p: 19937, note: "The output is the fixed 64-entry permutation, cycled: it repeats every 64 numbers." },
      { name: "sdcMicro record swapping", diff: "\\text{rank of } 1/E_j,\\; E_j = -\\ln(1 - u_j),\\; j < 16", where: "targeted record swapping (2021 census round)", file: "kinds/sdcmicro_recordswap_exp_race.c", kind: K.SDCMICRO, p: 19937 },
    );
    g("MT19937-64");
    R.push(
      { name: "MT19937-64", diff: "w = 64,\\; n = 312,\\; m = 156,\\; r = 31", where: "std::mt19937_64, Stata, SAS, Redis", file: "internal.c · 50 catalog entries", v: V.MT64, p: 19937 },
      { name: "Stim", diff: "s \\oplus \\texttt{0xDEADBEEF124C}", where: "quantumlib/Stim --seed", file: "kinds/stim_mt19937_64_versionseed_xor.c", v: V.STIM, p: 19937 },
      { name: "Apery", diff: "z \\land \\lnot 1", where: "shogi engine Zobrist keys", file: "kinds/apery_shogi_mt19937_64_evenmask.c", v: V.APERY, p: 19937 },
      { name: "Qiskit Aer", diff: "\\text{plain } \\mathrm{mt19937\\_64}(s) \\text{ per shot}", where: "IBM quantum simulator", file: "kinds/qiskit_aer_mt19937_64_shotseed.c", kind: K.QISKIT, p: 19937 },
      { name: "random_device → mt19937_64", diff: "s \\bmod 2^{32}", where: "key generation seeded with 32 bits (catalog: Profanity / Wintermute)", file: "kinds/profanity_wintermute_mt19937_64_randomdevice.c", kind: K.PROFANITY, p: 19937 },
      { name: "Quackle", diff: "z \\bmod 100,\\; \\text{reject } z \\ge 2^{64} - (2^{64} \\bmod 100)", where: "Scrabble engine tile draws", file: "kinds/quackle_mt19937_64_custom_rejection.c", kind: K.QUACKLE, p: 19937 },
    );
    g("Other sizes");
    R.push(
      { name: "MT11213B", diff: "n = 351,\\; m = 175,\\; r = 19", where: "boost::mt11213b", file: "internal_wave.c", v: V.MT11213B, p: 11213 },
      { name: "TT800", diff: "r = 0 \\text{ (twisted GFSR)},\\; n = 25", where: "GSL gsl_rng_tt800, ROOT TRandom2, OpenBUGS", file: "internal_wave.c", v: V.TT800, p: 800 },
      { name: "GNU Go TT800", diff: "\\text{no } y \\mathrel{\\oplus}= y \\gg 16;\\; s_{i+1} = 1313\\,s_i + 88897", where: "gg_urand, Zobrist hashing", file: "kinds/gnugo_tt800_lcgseed.c", v: V.GNUGO, p: 800 },
    );
    g("TinyMT");
    R.push(
      { name: "TinyMT32", diff: "127\\text{-bit state}", where: "Pokémon Gen 6, Nintendo Switch nn::util, RFC 8682", file: "internal.c · 10 catalog entries", tiny: TINY, src: S.TINYMT, p: 127 },
      { name: "rusEFI TinyMT32", diff: "\\mathit{mat}_1 = \\mathit{mat}_2 = \\mathit{tmat} = 0", where: "engine-control firmware", file: "kinds/rusefi_tinymt32_zeromat.c", tiny: { mat1: 0, mat2: 0, tmat: 0 }, src: S.TINYMT_ZERO, p: 127, note: "Without mat₁, mat₂ the recurrence is no longer the certified primitive one." },
      { name: "TinyMT64", diff: "127\\text{-bit state},\\; 64\\text{-bit output}", where: "MersenneTwister-Lab reference (check64)", file: "kinds/tinymt64.c", kind: K.TINYMT64, p: 127 },
      { name: "wrk2 TinyMT64", diff: "\\mathit{mat}_1 = \\mathit{mat}_2 = \\mathit{tmat} = 0 \\text{ (never set)}", where: "wrk2 HTTP load generator", file: "kinds/wrk2_tinymt64_degenerate_zeroparams.c", kind: K.WRK2, p: 127, note: "zcalloc leaves mat₁ = mat₂ = tmat = 0, so the recurrence is not the certified primitive one: its degree drops below 127." },
    );
    g("SFMT, all sizes");
    SFMT_EXP.forEach((e, i) => R.push({ name: "SFMT" + e, sfmtIdx: i, kind: K.SFMT0 + i, p: e, where: SFMT_USERS[e] || "SFMT reference code", file: e === 19937 ? "internal.c · 27 catalog entries" : "kinds/sfmt" + e + ".c" }));
    g("MELG (64-bit, maximally equidistributed)");
    MELG_EXP.forEach((e, i) => R.push({ name: "MELG" + e + "-64", melgIdx: i, kind: K.MELG0 + i, p: e, where: MELG_USERS[e] || "Harase & Kimoto reference code", file: "kinds/melg" + e + "_64.c" }));
    g("Relatives");
    R.push({ name: "XSadd", diff: "s_3 + s_2 \\text{ in place of tempering}", where: "ml-xsadd (ml.js)", file: "kinds/xsadd_saito_matsumoto.c", kind: K.XSADD, p: 128, note: "Not a twister: a 128-bit xorshift from the same lab. Its period is 2¹²⁸ − 1, not a Mersenne prime." });
    // fill in the parameter-dependent formulas
    for (const r of R) {
      if (r.sfmtIdx !== undefined) {
        const q = MTV.sfmtParams(r.sfmtIdx);
        r.sp = q;
        r.diff = "N = " + q.n + ",\\; \\mathit{POS1} = " + q.pos1 + ",\\; (" + q.sl1 + ", " + q.sl2 + ", " + q.sr1 + ", " + q.sr2 + ")";
      }
      if (r.melgIdx !== undefined) {
        const q = MTV.melgParams(r.melgIdx);
        r.mp = q;
        r.diff = "N = " + q.nn + ",\\; M = " + q.mm + ",\\; \\text{lag } " + q.lag1 + ",\\; " + q.nn + "\\cdot 64 - " + (64 - q.p) + " + 64 = " + q.mexp;
      }
    }
    return R;
  }

  // kind_info for a kind ≥ 100: state size, which bit is linear, the core and its seed
  function kindInfo(id, seed) {
    const s = BigInt.asUintN(64, BigInt(seed));
    X.kind_info(id, MTV.lo(s), MTV.hi(s));
    const a = Array.from(MTV.out32());
    const info = { stateBits: a[0], top: a[1], stride: a[2], bits: a[3], range: a[4], core: a[5], idx: a[6], coreSeed: MTV.j64(a[7], a[8]), mat1: a[9], mat2: a[10], tmat: MTV.j64(a[11], a[12]) };
    if (info.core === 1) info.cfg = MTV.decodeCfg(a.slice(13));
    return info;
  }

  // debugger entry for a table row
  function rowEntry(vr) {
    if (vr.v !== undefined) return { label: vr.name, prog: "mt", cfg: MTV.variant(vr.v), seed: 5489 };
    if (vr.tiny) return { label: vr.name, prog: "tiny", cfg: vr.tiny, seed: 1 };
    const ki = kindInfo(vr.kind, 5489);
    const seed = ki.coreSeed;
    const sub = seed !== 5489n ? " (core seed " + seed + ")" : "";
    switch (ki.core) {
      case 1: return { label: vr.name + sub, prog: "mt", cfg: ki.cfg, seed };
      case 2: return { label: vr.name, prog: "tiny64", cfg: { mat1: ki.mat1, mat2: ki.mat2, tmat: ki.tmat }, seed };
      case 3: return { label: vr.name, prog: "melg", cfg: MTV.melgParams(ki.idx), seed };
      case 4: return { label: vr.name, prog: "sfmt", cfg: MTV.sfmtParams(ki.idx), seed: 1234 };
      case 5: return { label: vr.name, prog: "xsadd", cfg: {}, seed: 1 };
    }
    return null;
  }

  // linear-complexity measurement plan for a row that is not an MT config
  function lcPlan(vr) {
    if (vr.tiny) return { kind: vr.src, nbits: 2 * 128 + 128, top: 0, stride: 1 };
    const ki = kindInfo(vr.kind, 5489);
    return { kind: vr.kind, nbits: 2 * ki.stateBits + 128, top: ki.top, stride: ki.stride };
  }

  function variantsSection() {
    const VARIANTS = variantRows();
    const nRows = VARIANTS.filter((r) => !r.group).length;
    $("#var-lede").innerHTML = "My prngs catalog (a private repo, sorry) has 5 122 generators in it, and a surprising number of them are Mersenne Twisters in disguise. 689 go straight to one of four cores: 602 × MT19937, 50 × MT19937-64, 27 × SFMT19937 and 10 × TinyMT32. 8 more run MT11213B or TT800, and 135 are their own kinds because somebody changed the seeding, the twist, the output or the size. Below are all " + nRows + " distinct ones. Every row is checked bit for bit against the C code (5 seeds, first 6 outputs and the 1000th), and where the original project's code is public there's a link to it.";
    const tb = $("#var-table");
    tb.innerHTML = "<tr><th>variant</th><th>what differs</th><th>used by</th><th>prngs (private repo)</th><th>upstream code</th><th title='Berlekamp–Massey on one output bit: the top bit (bit 0 for TinyMT and XSadd, whose outputs add two words; one word per 128-bit block for SFMT)'>deg</th><th></th></tr>";
    const lcCells = [], jobRows = [];
    for (const vr of VARIANTS) {
      if (vr.group) {
        tb.append(h("tr", { class: "grp" }, h("td", { colspan: "7" }, vr.group)));
        continue;
      }
      const diff = h("td"); MTV.tex(diff, vr.diff);
      const lc = h("td", { class: "m", style: "white-space:nowrap" }, "…");
      const dbgBtn = h("button", { onclick: () => { const e = rowEntry(vr); if (e) openInDebugger(e); } }, "Debug");
      const stBtn = h("button", { onclick: () => {
        const s = vr.v !== undefined ? { label: vr.name, kind: S.MT, cfg: MTV.variant(vr.v) } : vr.tiny ? { label: vr.name, kind: vr.src } : { label: vr.name, kind: vr.kind };
        MTV.addStatsSource(s, true);
        $("#stats").scrollIntoView();
      } }, "Distributions");
      tb.append(h("tr", null, h("td", null, vr.name), diff, h("td", { class: "small" }, vr.where), h("td", { class: "m small" }, vr.file), (() => { const u = upstream(vr); return h("td", { class: "small" }, u ? h("a", { href: u[0], target: "_blank", rel: "noopener" }, u[1] + " ↗") : "—"); })(), lc, h("td", { style: "white-space:nowrap" }, dbgBtn, " ", stBtn)));
      if (vr.v !== undefined) lcCells.push([vr, lc]);
      else jobRows.push([vr, lc]);
    }
    // filter box: hide rows that don't match, and group headers left empty
    const flt = $("#var-filter");
    const applyFilter = () => {
      const q = flt.value.trim().toLowerCase();
      let grp = null, any = false, shown = 0;
      const close = () => { if (grp) grp.style.display = any ? "" : "none"; };
      for (const tr of tb.querySelectorAll("tr")) {
        if (tr.classList.contains("grp")) { close(); grp = tr; any = false; continue; }
        if (!tr.querySelector("td")) continue;
        const ok = !q || tr.textContent.toLowerCase().includes(q) || (grp && grp.textContent.toLowerCase().includes(q));
        tr.style.display = ok ? "" : "none";
        if (ok) { any = true; shown++; }
      }
      close();
      $("#var-count").textContent = q ? shown + " of " + nRows + " rows" : nRows + " rows";
    };
    if (flt) { flt.oninput = applyFilter; applyFilter(); }
    const mark = (cell, vr, L, nbits) => {
      cell.textContent = String(L);
      cell.classList.remove("fail", "muted");
      if (nbits && 2 * L >= nbits - 64) {
        cell.textContent = "none";
        cell.classList.add("muted");
        cell.title = "L = " + L + " ≈ N/2 for N = " + nbits + " bits: no short linear recurrence. Rejection, sorting or a fixed table breaks GF(2)-linearity.";
        return;
      }
      if (L !== vr.p && vr.sp) {
        // by design, not a defect: shown plainly with an explanation
        cell.title = "SFMT's 128·" + vr.sp.n + " = " + 128 * vr.sp.n + "-bit state carries " + (128 * vr.sp.n - vr.p) + " dimensions outside the 2^" + vr.p + " − 1 cycle; period certification only guarantees the big factor is present, so the period is a multiple of 2^" + vr.p + " − 1.";
        cell.textContent = L + " *";
      } else if (L !== vr.p) {
        cell.classList.add("fail");
        if (vr.v === V.PPSSPP) cell.title = "Bit 31 of the raw words satisfies x[k+624] = x[k+397], a recurrence of degree 227; the whole output stream repeats every 454 numbers.";
        else if (vr.v === V.PHP_BROKEN) cell.title = "The bug feeds bit 0 of x_k back in, a bit the reference twist never uses, so the recurrence gains a dimension: 19938 ≠ p.";
        else cell.title = (vr.note || "") + " Expected " + vr.p + ".";
      } else if (vr.note) cell.title = vr.note;
    };
    whenVisible(tb, () => {
      for (const [vr, cell] of lcCells) {
        const c = MTV.variant(vr.v);
        const cp = charPoly(c, 5489);
        mark(cell, vr, cp.L, 0);
      }
      // the others run as background jobs, smallest first
      const plans = jobRows.map(([vr, cell]) => ({ vr, cell, plan: lcPlan(vr) })).sort((a, b) => a.plan.nbits - b.plan.nbits);
      let qi = 0, job = 0;
      const next = () => {
        if (qi >= plans.length) return;
        const { vr, cell, plan } = plans[qi];
        cell.textContent = "running";
        job = X.job_lc_new(plan.kind, 5489, 0, plan.nbits, plan.top, plan.stride);
        MTV.runJob(() => {
          const done = X.job_lc_step(job, 2048);
          const o = MTV.out32();
          if (!done && plan.nbits > 100000) cell.textContent = Math.round((100 * o[0]) / o[1]) + " %";
          if (done) { mark(cell, vr, o[2], plan.nbits); X.obj_free(job); }
          return done;
        }, () => { qi++; next(); }, 10);
      };
      next();
    });
    $("#var-notes").innerHTML = "";
    const n1 = h("p", { class: "small" });
    n1.innerHTML = "<b>deg</b> is the linear complexity of one output bit, measured live right now with Berlekamp–Massey on 2·(state bits) + 128 bits. For a healthy F₂-linear generator it equals the Mersenne exponent. Two MT ports actually break the math. <b>PPSSPP</b>: the lower mask equals the upper one, so <i>y</i> is always even and <i>A</i> never does anything. The low 30 bits just copy <i>x</i><sub>k+397</sub>, and the output repeats every <b>454</b> numbers, for every seed I tried. Ouch. <b>PHP MT_RAND_PHP</b> reads the low bit of <i>x</i><sub>k</sub> instead of <i>y</i>. That leaks a bit that's normally thrown away back into the recurrence, so the degree becomes 19 938 and the polynomial isn't the designed primitive one anymore.";
    const n2 = h("p", { class: "small" });
    n2.innerHTML = "<b>SFMT</b> (*) measures above its exponent, e.g. 19 968 = 128·156 for SFMT19937. That's on purpose, not a bug: the 128<i>N</i>-bit recursion has characteristic polynomial φ<sub>MEXP</sub>·ψ, and the period certification at seeding only makes sure the φ<sub>MEXP</sub> part is in there. So the period is a <i>multiple</i> of 2<sup>MEXP</sup> − 1. <b>wrk2</b> and <b>rusEFI</b> never set the TinyMT parameters, so their recurrences aren't the certified primitive ones (123 and 125 instead of 127). <b>GIMP</b> only ever outputs one fixed shuffle of 64 numbers, over and over. Rows saying <i>none</i> push the numbers through rejection or sorting, which isn't linear over F₂ at all. That's expected and fine.";
    $("#var-notes").append(n1, n2);
    $("#fam-tiny").onclick = () => openInDebugger({ label: "TinyMT32", prog: "tiny", cfg: TINY, seed: 1 });
    $("#fam-sfmt").onclick = () => openInDebugger({ label: "SFMT19937", prog: "sfmt", cfg: {}, seed: 1234 });
    const t64 = $("#fam-tiny64"); if (t64) t64.onclick = () => openInDebugger(rowEntry({ name: "TinyMT64", kind: K.TINYMT64 }));
    const mg = $("#fam-melg"); if (mg) mg.onclick = () => openInDebugger(rowEntry({ name: "MELG19937-64", kind: K.MELG0 + 5 }));
  }

  // ======================================================================== nav
  function navSpy() {
    const links = $$("#nav a");
    const map = new Map(links.map((a) => [a.getAttribute("href").slice(1), a]));
    const io = new IntersectionObserver((es) => {
      es.forEach((e) => {
        if (e.isIntersecting) { links.forEach((a) => a.classList.remove("on")); const a = map.get(e.target.id); if (a) a.classList.add("on"); }
      });
    }, { rootMargin: "-45% 0px -50% 0px" });
    $$("main section").forEach((s) => io.observe(s));
  }

  // ======================================================================== boot
  async function boot() {
    try {
      X = await MTV.load();
    } catch (e) {
      $("#loading").innerHTML = '<span class="err">Could not start WebAssembly: ' + e + "</span>";
      return;
    }
    MTV.renderAllTex();
    const steps = [hero, debuggerSection, twistSection, periodSection, temperingSection, buildSection, statsSection, weakSection, variantsSection, navSpy];
    for (const f of steps) {
      try { f(); } catch (e) { console.error(f.name, e); }
    }
    $("#loading").remove();
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", boot);
  else boot();
})();

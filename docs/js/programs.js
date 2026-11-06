/* programs.js — pseudocode for every steppable program. Line ids match crate/src/vm.rs. */
(function () {
  "use strict";
  const MTV = window.MTV;
  const X = () => MTV.X;
  const H = (v, w) => "\\texttt{" + MTV.hex(v, w) + "}";
  const K = (s) => "\\textbf{" + s + "}";
  const C = (s) => "\\quad\\textcolor{#8a919c}{\\triangleright\\ \\text{" + s + "}}";

  // ================================================================== MT (generic)
  const SEED_NAMES = ["init_genrand (2002)", "Knuth 69069 (1998)", "Cokus (x|1, ×69069)", "init_by_array (CPython)", "R set.seed (LCG 69069x+1)", "std::seed_seq", "GNU Go (1313x + 88897)", "Cokus, shifted"];
  const BULK_SEED = {
    3: (n, s0) => "x_{0..\\," + (n - 1) + "} \\gets \\mathrm{init\\_by\\_array}\\big(\\mathrm{key}(" + s0 + ")\\big)",
    4: (n, s0) => "x \\gets " + s0 + ";\\; 51\\times\\, x \\gets 69069x + 1;\\;\\; x_i \\gets (x \\gets 69069x + 1),\\ i = 0.." + (n - 1),
    5: (n, s0) => "x_{0..\\," + (n - 1) + "} \\gets \\mathrm{seed\\_seq}\\{" + s0 + "\\}.\\mathrm{generate}()",
    6: (n, s0) => "x_i \\gets s_i,\\;\\; s_0 = " + s0 + ",\\; s_{i+1} = 1313\\,s_i + 88897,\\ i = 0.." + (n - 1),
    7: (n, s0) => "x \\gets " + s0 + " \\lor 1;\\;\\; x_i \\gets (x \\gets 69069x),\\ i = 0.." + (n - 1),
  };

  function mtLines(c) {
    const w = c.w, n = c.n, m = c.m;
    const wm = MTV.mask(w);
    const L = [];
    const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
    add(0, 0, "", false);
    L[0].tex = K("function ") + "\\mathrm{seed}(s)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ \\text{all arithmetic mod } 2^{" + w + "}}";
    let s0 = "s";
    if (c.seedXor) s0 = "s \\oplus " + H(c.seedXor, 48);
    if (c.zeroSub) s0 = "(s = 0\\ ?\\ " + Number(c.zeroSub) + " : s)";
    if (c.seedMode === 2) add(1, 1, "x \\gets " + s0 + " \\lor 1;\\;\\; x_0 \\gets x");
    else if (BULK_SEED[c.seedMode]) add(1, 1, BULK_SEED[c.seedMode](n, s0) + C("one step"));
    else add(1, 1, "x_0 \\gets " + s0);
    const skipLoop = !!BULK_SEED[c.seedMode];
    add(2, 1, K("for ") + "i \\gets 1 " + K(" to ") + (n - 1));
    if (c.seedMode === 1) add(3, 2, "x_i \\gets 69069 \\cdot x_{i-1}");
    else if (c.seedMode === 2) add(3, 2, "x \\gets 69069 \\cdot x;\\;\\; x_i \\gets x");
    else add(3, 2, "x_i \\gets " + Number(c.f) + "\\cdot\\big(x_{i-1} \\oplus (x_{i-1} \\gg " + (w - 2) + ")\\big) + i");
    if (skipLoop) {
      // the loop back-edge is the last two entries, mark them to skip
      for (let k = L.length - 2; k < L.length; k++) L[k].skip = true;
    }
    add(4, 1, "\\mathit{idx} \\gets " + n);
    add(5, 0, K("function ") + "\\mathrm{next}()", false);
    add(6, 1, K("if ") + "\\mathit{idx} \\ge " + n + K(" then ") + "\\mathrm{twist}()");
    add(7, 1, "y \\gets x_{\\mathit{idx}};\\;\\; \\mathit{idx} \\gets \\mathit{idx}+1");
    const tline = (sh, dir, mask) => {
      if (!c.temper) return "y \\gets y" + C("tempering off");
      if (sh >= w) return "y \\gets y\\quad\\textcolor{#8a919c}{\\triangleright\\ \\text{step skipped: shift } " + sh + " \\ge w}";
      const op = dir === "r" ? "\\gg" : "\\ll";
      if (BigInt(mask) === wm) return "y \\gets y \\oplus (y " + op + " " + sh + ")";
      return "y \\gets y \\oplus \\big((y " + op + " " + sh + ") \\land " + H(mask, w) + "\\big)";
    };
    add(8, 1, tline(c.u, "r", c.d));
    add(9, 1, tline(c.s, "l", c.b));
    add(10, 1, tline(c.t, "l", c.c));
    add(11, 1, tline(c.l, "r", wm));
    let ret = "y";
    if (c.outShift) ret = "(" + ret + " \\gg " + c.outShift + ")";
    if (c.outXor) ret = ret + " \\oplus " + H(c.outXor, w);
    if (BigInt(c.outAnd) !== (wm >> BigInt(c.outShift || 0)) && BigInt(c.outAnd) !== wm) ret = "(" + ret + ") \\land " + H(c.outAnd, w);
    add(12, 1, K("return ") + ret + (c.discard ? C("first " + c.discard + " discarded") : ""));
    add(13, 0, K("function ") + "\\mathrm{twist}()", false);
    add(14, 1, K("for ") + "k \\gets 0 " + K(" to ") + (n - 1) + C("indices mod " + n));
    const um = BigInt(c.umask), lm = BigInt(c.lmask);
    let yl;
    if (c.r === 0 && lm === 0n) yl = "y \\gets x_k";
    else yl = "y \\gets (x_k \\land " + H(um, w) + ") + (x_{k+1} \\land " + H(lm, w) + ")";
    add(15, 2, yl);
    add(16, 2, "v \\gets y \\gg 1");
    add(17, 2, K("if ") + (c.lowbit ? "\\textcolor{#d14}{x_k}" : "y") + "\\text{ odd}: \\; v \\gets v \\oplus " + H(c.a, w));
    add(18, 2, "x_k \\gets x_{k+" + m + "} \\oplus v");
    add(19, 1, "\\mathit{idx} \\gets 0");
    return L;
  }

  function mtLabels(line, rows, c) {
    const w = c.w;
    switch (line) {
      case 1:
        if (rows === 1) return ["x_0"];
        return rows === 3 ? ["s", "s'", "x_0"] : ["s", "x_0"];
      case 3:
        if (rows === 5) return ["x_{i-1}", "\\gg " + (w - 2), "\\oplus", "\\cdot f", "+\\, i"];
        return ["x_{i-1}", "\\cdot 69069"];
      case 7: return ["x_{\\mathit{idx}}"];
      case 8: case 9: case 10: {
        if (rows === 2) return ["y", "y"];
        const sh = line === 8 ? "\\gg " + c.u : line === 9 ? "\\ll " + c.s : "\\ll " + c.t;
        const mk = line === 8 ? "d" : line === 9 ? "b" : "c";
        return ["y", "y " + sh, mk, "\\land", "y\\,\\oplus"];
      }
      case 11: return rows === 2 ? ["y", "y"] : ["y", "y \\gg " + c.l, "y\\,\\oplus"];
      case 12: return ["y", "\\mathrm{out}"];
      case 15: return ["x_k", "\\text{upper}", "x_{k+1}", "\\text{lower}", "y"];
      case 16: return ["y", "y \\gg 1"];
      case 17: return ["v", "a \\text{ or } 0", "v"];
      case 18: return ["x_{k+" + c.m + "}", "v", "x_k"];
    }
    return [];
  }
  const mtReadRows = { 3: [0], 7: [0], 15: [0, 2], 18: [0] };
  // operator rows: rows whose label is an operator rather than a variable
  const mtRules = { 3: 4, 8: 4, 9: 4, 10: 4, 11: 2, 15: 4, 17: 2, 18: 2 };

  const mt = {
    key: "mt",
    create(c, seed) {
      MTV.setCfg(c);
      const s = BigInt.asUintN(64, BigInt(seed));
      return X().vm_new_mt(MTV.lo(s), MTV.hi(s));
    },
    lines: mtLines,
    labels: mtLabels,
    readRows: mtReadRows,
    ruleBefore: mtRules,
    loops: [[2, 3], [14, 18]],
    width: (c) => c.w,
    frames(hdr) {
      const pc = hdr.pc;
      if (pc <= 4) return ["seed"];
      if (pc >= 13) return ["next", "twist"];
      return ["next"];
    },
    watch(info, hdr, c) {
      const y = MTV.j64(info[3], info[4]), v = MTV.j64(info[5], info[6]);
      const pc = hdr.pc;
      const rows = [];
      if (pc <= 4) rows.push(["i", info[0]]);
      rows.push(["k", info[1]], ["\\mathit{idx}", info[2]], ["y", MTV.hex(y, c.w)], ["v", MTV.hex(v, c.w)]);
      rows.push(["\\text{twists}", info[9]]);
      if (c.discard) rows.push(["\\text{to discard}", info[10]]);
      rows.push(["\\text{steps}", MTV.fmtInt(hdr.steps)], ["\\text{outputs}", MTV.fmtInt(hdr.outCount)]);
      return rows;
    },
    inline(line, hdr, info, c, last) {
      const w = c.w;
      if (line === 2) return "i = " + info[0];
      if (line === 14) return "k = " + info[1];
      if (line === 6) return info[2] >= c.n ? "twist" : "idx = " + info[2];
      if (line === 4 || line === 19) return "idx = " + info[2];
      if (last != null) return "= " + MTV.hex(last, w);
      return "";
    },
    seedName: (c) => SEED_NAMES[c.seedMode],
  };

  // ================================================================== TinyMT32
  const tiny = {
    key: "tiny",
    create(c, seed) {
      return X().vm_new_tiny(Number(BigInt.asUintN(32, BigInt(seed))), c.mat1 >>> 0, c.mat2 >>> 0, c.tmat >>> 0);
    },
    lines(c) {
      const L = [];
      const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
      add(0, 0, K("function ") + "\\mathrm{init}(s)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ \\text{state } (s_0,s_1,s_2,s_3),\\ 127 \\text{ bits}}", false);
      add(1, 1, "(s_0,s_1,s_2,s_3) \\gets (s,\\ " + H(c.mat1, 32) + ",\\ " + H(c.mat2, 32) + ",\\ " + H(c.tmat, 32) + ")");
      add(2, 1, K("for ") + "i \\gets 1..7:\\ s_{i \\bmod 4} \\mathrel{\\oplus}= i + 1812433253\\cdot(s_{i-1} \\oplus (s_{i-1} \\gg 30))");
      add(3, 1, K("repeat ") + "8\\times:\\ \\mathrm{next\\_state}()");
      add(4, 0, K("function ") + "\\mathrm{next}()", false);
      add(5, 1, "y \\gets s_3");
      add(6, 1, "x \\gets (s_0 \\land \\texttt{0x7fffffff}) \\oplus s_1 \\oplus s_2");
      add(7, 1, "x \\gets x \\oplus (x \\ll 1)");
      add(8, 1, "y \\gets y \\oplus (y \\gg 1) \\oplus x");
      add(9, 1, "(s_0, s_1, s_2, s_3) \\gets (s_1,\\ s_2,\\ x \\oplus (y \\ll 10),\\ y)");
      add(10, 1, K("if ") + "y \\text{ odd}:\\ s_1 \\mathrel{\\oplus}= \\mathit{mat}_1;\\ s_2 \\mathrel{\\oplus}= \\mathit{mat}_2");
      add(11, 1, "t_1 \\gets s_0 + (s_2 \\gg 8)" + C("tempering"));
      add(12, 1, "t_0 \\gets s_3 \\oplus t_1");
      add(13, 1, K("if ") + "t_1 \\text{ odd}:\\ t_0 \\gets t_0 \\oplus \\mathit{tmat}");
      add(14, 1, K("return ") + "t_0");
      return L;
    },
    labels(line, rows) {
      switch (line) {
        case 1: case 2: case 3: return ["s_0", "s_1", "s_2", "s_3"];
        case 5: return ["y"];
        case 6: return ["s_0 \\land m", "s_1", "s_2", "x"];
        case 7: return ["x", "x \\ll 1", "x"];
        case 8: return ["y", "y \\gg 1", "x", "y"];
        case 9: return ["x", "y \\ll 10", "s_2"];
        case 10: return ["\\mathit{mat}_1", "s_1", "\\mathit{mat}_2", "s_2"];
        case 11: return ["s_0", "s_2 \\gg 8", "t_1"];
        case 12: return ["s_3", "t_1", "t_0"];
        case 13: return ["t_0", "\\mathit{tmat}", "t_0"];
        case 14: return ["t_0"];
      }
      return [];
    },
    readRows: { 6: [1, 2], 11: [0], 12: [0] },
    ruleBefore: { 6: 3, 7: 2, 8: 3, 9: 2, 11: 2, 12: 2, 13: 2 },
    loops: [],
    width: () => 32,
    frames: (hdr) => (hdr.pc <= 3 ? ["init"] : ["next"]),
    watch(info, hdr) {
      return [
        ["s_0", MTV.hex(info[0], 32)], ["s_1", MTV.hex(info[1], 32)], ["s_2", MTV.hex(info[2], 32)], ["s_3", MTV.hex(info[3], 32)],
        ["x", MTV.hex(info[4], 32)], ["y", MTV.hex(info[5], 32)], ["t_0", MTV.hex(info[6], 32)], ["t_1", MTV.hex(info[7], 32)],
        ["\\text{outputs}", MTV.fmtInt(hdr.outCount)],
      ];
    },
    inline(line, hdr, info, c, last) {
      return last != null ? "= " + MTV.hex(last, 32) : "";
    },
  };

  // ================================================================== SFMT (any MEXP)
  // c = { idx, mexp, n, pos1, sl1, sl2, sr1, sr2 } (from sfmt_params); {} = SFMT19937
  const SF19937 = { idx: 5, mexp: 19937, n: 156, pos1: 122, sl1: 18, sl2: 1, sr1: 11, sr2: 1 };
  const sfp = (c) => (c && c.n ? c : SF19937);
  MTV.sfmtParams = function (idx) {
    X().sfmt_params(idx);
    const a = Array.from(MTV.out32());
    return { idx, mexp: a[0], n: a[1], pos1: a[2], sl1: a[3], sl2: a[4], sr1: a[5], sr2: a[6], msk: a.slice(7, 11), parity: a.slice(11, 15) };
  };
  const sfmtp = {
    key: "sfmt",
    create(c, seed) {
      return X().vm_new_sfmt_p(sfp(c).idx, Number(BigInt.asUintN(32, BigInt(seed))));
    },
    lines(c0) {
      const c = sfp(c0), n = c.n, N32 = 4 * n;
      const L = [];
      const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
      add(0, 0, K("function ") + "\\mathrm{init}(s)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ W_0 \\dots W_{" + (n - 1) + "}: 128\\text{-bit blocks},\\ \\mathit{MEXP} = " + c.mexp + "}", false);
      add(1, 1, "W \\gets \\mathrm{init\\_genrand}(s)" + C(N32 + " words"));
      add(2, 1, "\\mathrm{period\\_certification}(W)" + C("leave the bad subspace"));
      add(3, 0, K("function ") + "\\mathrm{next}()", false);
      add(4, 1, K("if ") + "\\mathit{idx} \\ge " + N32 + " " + K(" then ") + "\\mathrm{gen\\_all}()");
      add(5, 1, K("return ") + "w_{\\mathit{idx}{+}{+}}" + C("32-bit view of W"));
      add(6, 0, K("function ") + "\\mathrm{gen\\_all}()" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ \\ll " + 8 * c.sl2 + ",\\ \\gg " + 8 * c.sr2 + " \\text{ shift the 128-bit block; } \\gg " + c.sr1 + ",\\ \\ll " + c.sl1 + " \\text{ each 32-bit word}}", false);
      add(7, 1, "r_1 \\gets W_{" + (n - 2) + "};\\;\\; r_2 \\gets W_{" + (n - 1) + "}");
      add(8, 1, K("for ") + "i \\gets 0 " + K(" to ") + (n - 1));
      add(9, 2, "a \\gets W_i;\\;\\; b \\gets W_{i+" + c.pos1 + "}");
      add(10, 2, "W_i \\gets a \\oplus (a \\ll " + 8 * c.sl2 + ") \\oplus \\big((b \\gg " + c.sr1 + ") \\land M\\big) \\oplus (r_1 \\gg " + 8 * c.sr2 + ") \\oplus (r_2 \\ll " + c.sl1 + ")");
      add(11, 2, "r_1 \\gets r_2;\\;\\; r_2 \\gets W_i");
      add(12, 1, "\\mathit{idx} \\gets 0");
      return L;
    },
    labels(line, rows, c0) {
      const c = sfp(c0);
      switch (line) {
        case 1: return ["W_0"];
        case 2: return ["W_0", "\\text{parity}", "W_0"];
        case 5: return ["w"];
        case 7: return ["r_1", "r_2"];
        case 9: return ["a", "b"];
        case 10: return ["a", "a \\overset{128}{\\ll} " + 8 * c.sl2, "(b \\gg " + c.sr1 + ") \\land M", "r_1 \\overset{128}{\\gg} " + 8 * c.sr2, "r_2 \\overset{32}{\\ll} " + c.sl1, "W_i"];
        case 11: return ["r_1", "r_2"];
      }
      return [];
    },
    readRows: { 9: [0, 1], 7: [0, 1] },
    ruleBefore: { 10: 5 },
    loops: [[8, 11]],
    width: () => 32,
    frames: (hdr) => (hdr.pc <= 2 ? ["init"] : hdr.pc >= 6 ? ["next", "gen_all"] : ["next"]),
    watch(info, hdr) {
      return [["i", info[0]], ["\\mathit{idx}", info[1]], ["\\text{certification flipped a bit}", info[2] ? "yes" : "no"], ["\\text{outputs}", MTV.fmtInt(hdr.outCount)], ["\\text{steps}", MTV.fmtInt(hdr.steps)]];
    },
    inline(line, hdr, info, c0, last) {
      const N32 = 4 * sfp(c0).n;
      if (line === 8) return "i = " + info[0];
      if (line === 4) return info[1] >= N32 ? "gen_all" : "idx = " + info[1];
      return "";
    },
    stateName: "W",
    stateWord(d, i) {
      const n = sfp(d.cfg).n;
      if (i >= n) return null;
      const a = MTV.u32at(X().vm_state_ptr(d.handle), 4 * n);
      return { v: (BigInt(a[4 * i + 3]) << 96n) | (BigInt(a[4 * i + 2]) << 64n) | (BigInt(a[4 * i + 1]) << 32n) | BigInt(a[4 * i]), bits: 128 };
    },
    setWord(d, i, nv, b) {
      const word = i * 4 + Math.floor(b / 32);
      const cur = (nv >> BigInt(32 * Math.floor(b / 32))) & 0xffffffffn;
      X().vm_set_word(d.handle, word, Number(cur), 0);
    },
    stateModel(d) {
      const c = sfp(d.cfg), n = c.n, hd = d.hd;
      const a = MTV.u32at(X().vm_state_ptr(d.handle), 4 * n);
      const i0 = hd.info[0], inGen = hd.pc >= 8 && hd.pc <= 11;
      return {
        n, w: 128, kind: "ring", name: "W",
        word: (i) => (BigInt(a[4 * i + 3]) << 96n) | (BigInt(a[4 * i + 2]) << 64n) | (BigInt(a[4 * i + 1]) << 32n) | BigInt(a[4 * i]),
        frac: (i) => a[4 * i + 3] / 4294967296,
        fresh: (i) => inGen && i < i0,
        ptr: inGen ? -1 : Math.floor(hd.info[1] / 4),
        chords: inGen ? [[i0 % n, (i0 + c.pos1) % n], [i0 % n, (i0 + n - 2) % n], [i0 % n, (i0 + n - 1) % n]] : [],
        title: n + " blocks × 128 bits",
        bits: 128,
      };
    },
    centerText(d) {
      const hd = d.hd;
      if (hd.pc >= 8 && hd.pc <= 11) return ["gen_all · i = " + hd.info[0], "", ""];
      return ["idx = " + hd.info[1], "block " + Math.floor(hd.info[1] / 4), ""];
    },
  };

  // ================================================================== TinyMT64
  // c = { mat1, mat2, tmat (BigInt) }
  const tiny64 = {
    key: "tiny64",
    create(c, seed) {
      const s = BigInt.asUintN(64, BigInt(seed)), t = BigInt.asUintN(64, BigInt(c.tmat));
      return X().vm_new_tiny64(MTV.lo(s), MTV.hi(s), c.mat1 >>> 0, c.mat2 >>> 0, MTV.lo(t), MTV.hi(t));
    },
    lines(c) {
      const L = [];
      const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
      add(0, 0, K("function ") + "\\mathrm{init}(s)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ \\text{state } (s_0,s_1),\\ 127 \\text{ bits; arithmetic mod } 2^{64}}", false);
      add(1, 1, "(s_0,s_1) \\gets \\big(s \\oplus (" + H(c.mat1, 32) + " \\ll 32),\\ " + H(c.mat2, 32) + " \\oplus " + H(c.tmat, 64) + "\\big)");
      add(2, 1, K("for ") + "i \\gets 1..7:\\ s_{i \\bmod 2} \\mathrel{\\oplus}= i + 6364136223846793005\\cdot(s_{i-1} \\oplus (s_{i-1} \\gg 62))");
      add(3, 0, K("function ") + "\\mathrm{next}()", false);
      add(4, 1, "s_0 \\gets s_0 \\land \\texttt{0x7fff\\dots f};\\;\\; x \\gets s_0 \\oplus s_1");
      add(5, 1, "x \\gets x \\oplus (x \\ll 12)");
      add(6, 1, "x \\gets x \\oplus (x \\gg 32)");
      add(7, 1, "x \\gets x \\oplus (x \\ll 32)");
      add(8, 1, "x \\gets x \\oplus (x \\ll 11)");
      add(9, 1, "(s_0, s_1) \\gets (s_1,\\ x)");
      add(10, 1, K("if ") + "x \\text{ odd}:\\ s_0 \\mathrel{\\oplus}= \\mathit{mat}_1;\\ s_1 \\mathrel{\\oplus}= \\mathit{mat}_2 \\ll 32");
      add(11, 1, "t \\gets s_0 + s_1" + C("tempering; + is the only non-linear step"));
      add(12, 1, "t \\gets t \\oplus (s_0 \\gg 8)");
      add(13, 1, K("if ") + "t \\text{ odd}:\\ t \\gets t \\oplus \\mathit{tmat}");
      add(14, 1, K("return ") + "t");
      return L;
    },
    labels(line) {
      switch (line) {
        case 1: case 2: case 9: return ["s_0", "s_1"];
        case 4: return ["s_0", "s_1", "x"];
        case 5: return ["x", "x \\ll 12", "x"];
        case 6: return ["x", "x \\gg 32", "x"];
        case 7: return ["x", "x \\ll 32", "x"];
        case 8: return ["x", "x \\ll 11", "x"];
        case 10: return ["\\mathit{mat}_1", "s_0", "\\mathit{mat}_2 \\ll 32", "s_1"];
        case 11: return ["s_0", "s_1", "t"];
        case 12: return ["t", "s_0 \\gg 8", "t"];
        case 13: return ["t", "\\mathit{tmat}", "t"];
        case 14: return ["t"];
      }
      return [];
    },
    readRows: { 4: [0, 1], 11: [0, 1] },
    ruleBefore: { 4: 2, 5: 2, 6: 2, 7: 2, 8: 2, 11: 2, 12: 2, 13: 2 },
    loops: [],
    width: () => 64,
    frames: (hdr) => (hdr.pc <= 2 ? ["init"] : ["next"]),
    watch(info, hdr) {
      const g = (k) => MTV.hex(MTV.j64(info[2 * k], info[2 * k + 1]), 64);
      return [["s_0", g(0)], ["s_1", g(1)], ["x", g(2)], ["t", g(3)], ["\\text{outputs}", MTV.fmtInt(hdr.outCount)]];
    },
    inline(line, hdr, info, c, last) {
      return last != null ? "= " + MTV.hex(last, 64) : "";
    },
    stateName: "s",
    stateWord(d, i) {
      if (i > 1) return null;
      return { v: MTV.j64(d.hd.info[2 * i], d.hd.info[2 * i + 1]), bits: 64 };
    },
    stateModel(d) {
      const s = d.hd.info;
      const word = (i) => MTV.j64(s[2 * i], s[2 * i + 1]);
      return { n: 2, w: 64, kind: "reg", name: "s", word, frac: (i) => s[2 * i + 1] / 4294967296, fresh: () => false, ptr: -1, chords: [], title: "2 words, 127 effective bits", bits: 64 };
    },
  };

  // ================================================================== MELG
  // c = { idx, mexp, nn, mm, p, lag1, shift1, neg, pos, a, mask1 }
  MTV.melgParams = function (idx) {
    X().melg_params(idx);
    const a = Array.from(MTV.out32());
    return { idx, mexp: a[0], nn: a[1], mm: a[2], p: a[3], lag1: a[4], shift1: a[5], neg: a[6], pos: a[7], a: MTV.j64(a[8], a[9]), mask1: MTV.j64(a[10], a[11]) };
  };
  const melgp = {
    key: "melg",
    create(c, seed) {
      const s = BigInt.asUintN(64, BigInt(seed));
      return X().vm_new_melg(c.idx, MTV.lo(s), MTV.hi(s));
    },
    lines(c) {
      const L = [];
      const nn = c.nn;
      const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
      add(0, 0, K("function ") + "\\mathrm{seed}(s)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ x_0..x_{" + (nn - 1) + "} \\text{ and one more word, the lung } \\ell;\\ " + nn + "\\cdot 64 - " + (64 - c.p) + " + 64 = " + c.mexp + "}", false);
      add(1, 1, "x_0 \\gets s");
      add(2, 1, K("for ") + "i \\gets 1 " + K(" to ") + (nn - 1) + ":\\ x_i \\gets 6364136223846793005\\cdot(x_{i-1} \\oplus (x_{i-1} \\gg 62)) + i");
      add(3, 1, "\\ell \\gets 6364136223846793005\\cdot(x_{" + (nn - 1) + "} \\oplus (x_{" + (nn - 1) + "} \\gg 62)) + " + nn + ";\\;\\; i \\gets 0");
      add(4, 0, K("function ") + "\\mathrm{next}()" + C("indices mod " + nn), false);
      add(5, 1, "x \\gets (x_i \\land \\text{upper " + c.p + "}) \\lor (x_{i+1} \\land \\text{lower " + (64 - c.p) + "})");
      add(6, 1, "\\ell \\gets xA \\oplus x_{i+" + c.mm + "} \\oplus \\ell \\oplus (\\ell \\ll " + c.neg + ")" + C("xA = (x ≫ 1) ⊕ (x odd ? a : 0)"));
      add(7, 1, "x_i \\gets x \\oplus \\ell \\oplus (\\ell \\gg " + c.pos + ")");
      add(8, 1, "z \\gets x_i \\oplus (x_i \\ll " + c.shift1 + ")" + C("output transform, no tempering"));
      add(9, 1, "z \\gets z \\oplus (x_{i+" + c.lag1 + "} \\land " + H(c.mask1, 64) + ")");
      add(10, 1, "i \\gets i + 1;\\;\\; " + K("return ") + "z");
      return L;
    },
    labels(line, rows, c) {
      switch (line) {
        case 1: return ["x_0"];
        case 2: return ["x_{i-1}", "\\gg 62", "x_i"];
        case 3: return ["x_{" + (c.nn - 1) + "}", "\\ell"];
        case 5: return ["x_i", "\\text{upper}", "x_{i+1}", "\\text{lower}", "x"];
        case 6: return ["xA", "x_{i+" + c.mm + "}", "\\ell \\oplus (\\ell \\ll " + c.neg + ")", "\\ell"];
        case 7: return ["x", "\\ell \\oplus (\\ell \\gg " + c.pos + ")", "x_i"];
        case 8: return ["x_i", "x_i \\ll " + c.shift1, "z"];
        case 9: return ["z", "x_{i+" + c.lag1 + "}", "\\mathit{mask}", "z"];
        case 10: return ["z"];
      }
      return [];
    },
    readRows: { 2: [0], 5: [0, 2], 6: [1], 9: [1] },
    ruleBefore: { 2: 2, 5: 4, 6: 3, 7: 2, 8: 2, 9: 3 },
    loops: [[2, 2]],
    width: () => 64,
    frames: (hdr) => (hdr.pc <= 3 ? ["seed"] : ["next"]),
    watch(info, hdr, c) {
      const g = (k) => MTV.hex(MTV.j64(info[2 + 2 * k], info[3 + 2 * k]), 64);
      return [["i", info[1]], ["x", g(0)], ["z", g(1)], ["\\ell", g(2)], ["\\text{outputs}", MTV.fmtInt(hdr.outCount)], ["\\text{steps}", MTV.fmtInt(hdr.steps)]];
    },
    inline(line, hdr, info, c, last) {
      if (line === 2) return "i = " + info[0];
      if (line === 5) return "i = " + info[1];
      return last != null ? "= " + MTV.hex(last, 64) : "";
    },
    stateName: "x",
    stateWord(d, i) {
      const nn = d.cfg.nn;
      if (i > nn) return null;
      if (i === nn) { const s = d.hd.info; return { v: MTV.j64(s[6], s[7]), bits: 64 }; }
      const a = MTV.u32at(X().vm_state_ptr(d.handle), 2 * nn);
      return { v: MTV.j64(a[2 * i], a[2 * i + 1]), bits: 64 };
    },
    stateModel(d) {
      const c = d.cfg, nn = c.nn, hd = d.hd, s = hd.info;
      const a = MTV.u32at(X().vm_state_ptr(d.handle), 2 * nn);
      const lung = MTV.j64(s[6], s[7]);
      const word = (i) => (i === nn ? lung : MTV.j64(a[2 * i], a[2 * i + 1]));
      const i0 = s[1], inNext = hd.pc >= 5;
      return {
        n: nn + 1, w: 64, kind: nn > 40 ? "ring" : "reg", name: "x",
        label: (i) => (i === nn ? "ℓ" : "x" + i),
        word,
        frac: (i) => (i === nn ? s[7] : a[2 * i + 1]) / 4294967296,
        fresh: () => false,
        ptr: inNext ? i0 : -1,
        chords: inNext ? [[i0, (i0 + 1) % nn], [i0, (i0 + c.mm) % nn], [i0, (i0 + c.lag1) % nn], [i0, nn]] : [],
        title: nn + " words + lung × 64 bits",
        bits: 64,
      };
    },
    centerText(d) {
      const hd = d.hd;
      if (hd.pc <= 3) return ["seeding", "i = " + hd.info[0], ""];
      return ["i = " + hd.info[1], "ℓ = " + MTV.hex(MTV.j64(hd.info[6], hd.info[7]), 64).slice(0, 12) + "…", ""];
    },
  };

  // ================================================================== XSadd
  const xsaddp = {
    key: "xsadd",
    create(c, seed) {
      return X().vm_new_xsadd(Number(BigInt.asUintN(32, BigInt(seed))));
    },
    lines() {
      const L = [];
      const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
      add(0, 0, K("function ") + "\\mathrm{init}(s)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ 128\\text{-bit state, period } 2^{128}-1}", false);
      add(1, 1, "(s_0,s_1,s_2,s_3) \\gets (s,\\ 0,\\ 0,\\ 0)");
      add(2, 1, K("for ") + "i \\gets 1..7:\\ s_{i \\bmod 4} \\mathrel{\\oplus}= i + 1812433253\\cdot(s_{i-1} \\oplus (s_{i-1} \\gg 30))");
      add(3, 1, K("repeat ") + "8\\times:\\ \\mathrm{next\\_state}()");
      add(4, 0, K("function ") + "\\mathrm{next}()", false);
      add(5, 1, "t \\gets s_0 \\oplus (s_0 \\ll 15)");
      add(6, 1, "t \\gets t \\oplus (t \\gg 18)");
      add(7, 1, "t \\gets t \\oplus (s_3 \\ll 11)");
      add(8, 1, "(s_0,s_1,s_2,s_3) \\gets (s_1,\\ s_2,\\ s_3,\\ t)");
      add(9, 1, K("return ") + "s_3 + s_2" + C("addition instead of tempering"));
      return L;
    },
    labels(line) {
      switch (line) {
        case 1: case 2: case 3: case 8: return ["s_0", "s_1", "s_2", "s_3"];
        case 5: return ["s_0", "s_0 \\ll 15", "t"];
        case 6: return ["t", "t \\gg 18", "t"];
        case 7: return ["t", "s_3 \\ll 11", "t"];
        case 9: return ["s_3", "s_2", "s_3 + s_2"];
      }
      return [];
    },
    readRows: { 5: [0], 7: [1], 9: [0, 1] },
    ruleBefore: { 5: 2, 6: 2, 7: 2, 9: 2 },
    loops: [],
    width: () => 32,
    frames: (hdr) => (hdr.pc <= 3 ? ["init"] : ["next"]),
    watch(info, hdr) {
      return [["s_0", MTV.hex(info[0], 32)], ["s_1", MTV.hex(info[1], 32)], ["s_2", MTV.hex(info[2], 32)], ["s_3", MTV.hex(info[3], 32)], ["t", MTV.hex(info[4], 32)], ["\\text{outputs}", MTV.fmtInt(hdr.outCount)]];
    },
    inline(line, hdr, info, c, last) {
      return last != null ? "= " + MTV.hex(last, 32) : "";
    },
    stateName: "s",
    stateWord(d, i) {
      return i < 4 ? { v: BigInt(d.hd.info[i] >>> 0), bits: 32 } : null;
    },
    stateModel(d) {
      const s = d.hd.info;
      return { n: 4, w: 32, kind: "reg", name: "s", word: (i) => BigInt(s[i] >>> 0), frac: (i) => s[i] / 4294967296, fresh: () => false, ptr: -1, chords: [], title: "4 words × 32 bits", bits: 32 };
    },
  };

  // ================================================================== untemper
  const unt = {
    key: "unt",
    create(c, z) {
      MTV.setCfg(c);
      const s = BigInt.asUintN(64, BigInt(z));
      return X().vm_new_unt(MTV.lo(s), MTV.hi(s));
    },
    lines(c) {
      const w = c.w;
      const L = [];
      const add = (id, indent, tex, exec = true) => L.push({ id, indent, tex, exec });
      const reps = (k) => (k === 0 || k >= w ? 0 : Math.ceil(w / k) - 1);
      add(0, 0, K("function ") + "\\mathrm{untemper}(z)" + "\\quad\\textcolor{#8a919c}{\\triangleright\\ \\text{undo the four steps in reverse order}}", false);
      add(1, 1, "y \\gets z");
      const inv = (id, k, dir, mask, name) => {
        const op = dir === "r" ? "\\gg" : "\\ll";
        const mk = BigInt(mask) === MTV.mask(w) ? "" : " \\land " + name;
        const n = reps(k);
        if (n === 0) add(id, 1, "y \\gets y" + C("nothing to undo"));
        else add(id, 1, "x \\gets y;\\;\\; " + K("repeat ") + n + "\\times:\\ x \\gets y \\oplus \\big((x " + op + " " + k + ")" + mk + "\\big);\\;\\; y \\gets x");
      };
      inv(2, c.l, "r", MTV.mask(w), "");
      inv(3, c.t, "l", c.c, "c");
      inv(4, c.s, "l", c.b, "b");
      inv(5, c.u, "r", c.d, "d");
      add(6, 1, K("return ") + "y" + C("= the raw state word"));
      return L;
    },
    labels(line, rows) {
      if (line === 1) return ["y"];
      if (line === 6) return ["y"];
      if (rows === 3) return ["y", "y", "\\text{known}"];
      return ["y", "x", "\\text{term}", "y \\oplus \\text{term}", "\\text{known}"];
    },
    readRows: {},
    ruleBefore: { 2: 3, 3: 3, 4: 3, 5: 3 },
    loops: [[2, 2], [3, 3], [4, 4], [5, 5]],
    width: (c) => c.w,
    frames: () => ["untemper"],
    watch(info, hdr, c) {
      return [["z", MTV.hex(MTV.j64(info[0], info[1]), c.w)], ["y", MTV.hex(MTV.j64(info[2], info[3]), c.w)], ["x", MTV.hex(MTV.j64(info[4], info[5]), c.w)], ["j", info[6] + " / " + info[7]]];
    },
    inline(line, hdr, info, c, last) {
      return last != null ? "= " + MTV.hex(last, c.w) : "";
    },
  };

  MTV.PROGS = { mt, tiny, sfmt: sfmtp, unt, tiny64, melg: melgp, xsadd: xsaddp };
  MTV.SEED_NAMES = SEED_NAMES;
})();

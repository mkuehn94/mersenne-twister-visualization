# Mersenne Twister, taken apart

[Mersenne Twister](https://dl.acm.org/doi/10.1145/272991.272995) Is one of the most used Pseudorandom number generators or to be correct a FAMILY of generators. Since PNRGs fascinate me and I wanted to improve my rust & statistics I developed this repo. It's meant for visual/interactive learners who get bored reading houndreds of pages of specification/documentation and just want to see the thing in action, play with it, debug it, understand the probabilistic properties that make this class of generators so famous in computer science etc.

The result is live below, a static webpage using WebAssembly & Rust for maximum efficiency

**Live: https://mkuehn94.github.io/mersenne-twister-visualization/**

Paper (originally developped by Makoto Matsumoto and Takuji Nishimura) from 1998: https://dl.acm.org/doi/10.1145/272991.272995

Or offline: open `www/index.html` directly (the wasm is base64-embedded in `js/wasm_bytes.js`, no server needed). Or if you prefer:

```sh
python3 -m http.server -d www 8000
```

## What's on the page

- **Debugger** — pretty much gdb for pseudocode / twist / temper line by line. Ring view of the 624 state words with read/write highlights and the k+1 or relevant numbers are always highlighted. A small bit shows every operand of the last executed line. You can even modify bits mid generation and see what happens, like in true debugger fashion. Runs MT19937, MT19937-64, TT800, TinyMT, SFMT (all sizes), MELG, XSadd, and custom configs.
- **Twist** — the matrix A, block-companion B understanding the special operant F that is used in MT, characteristic polynomial.
- **Period** — This is one VERY important metric for a PRNG, basically repeatability. explanation of the very specific number 19937, and even proving irreduciblity live in your own browser.
- **Tempering** — Understanding one of the fundamental operations in MT: tempting. See how modifications alter the result.
- **Build your own** — One of the more fun parts, build your own completely new MR variant by searching for appropiate w,n,m,r params, search for a valid twist vector a (BM → sieve → irreducibility test, shown as a funnel)
- **Distributions** — A lot of important statistical properies visualized: χ² histograms, Box–Muller, CLT vs Irwin–Hall, 3-D lag plots (you can see RANDU's planes), bit raster, autocorrelation, Monte-Carlo π. Works with any source including broken ports. if some of those sound unfamiliar to you, no worries, I try to make it clear with the plots.
- **Notation** — cheat sheet for the symbols used on the page.
- **Weak spots** — Also a fun but important part that shows how the state can be recovered from 624 outputs (or however many the state vector holds). This HAS been used and WILL be probably used in the future, because one you have all the parameters you can now perfectly keep predicting the SAME numbers. Thats the exact weakness that makes a generator cryptographically insecure.
- **Variants** — 54 MT-family generators I scrapped from the internet with live linear complexity. Any of those can be tested interactivly.

## Layout

```
crate/            rust core. no_std + alloc, zero deps, plain C ABI (no wasm-bindgen)
www/              page itself, HTML/CSS/JS, KaTeX locally, mtviz.wasm
tools/cref/       compiles the prngs C code into ref.txt for cargo test
build.sh          one-shot: builds crate, wasm, and encodes as base64
```

## Build and test

```sh
rustup target add wasm32-unknown-unknown
./build.sh                        # rebuild wasm + embed
cd crate && cargo test --release  # 25 tests, 290 reference lines from the prngs C harness
```

`tools/cref/regen.sh <path-to-prngs>` regenerates `ref.txt` if you have the repo locally.

The Rust implementations are bit-exact ports of `prngs-main/dev/c/families/internal.c` and `kinds/*.c`. (private repo on my gitlab)

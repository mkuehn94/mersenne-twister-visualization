#!/usr/bin/env bash
# Regenerate ref.txt from the prngs repo's own C code (used by `cargo test`).
#   usage: tools/cref/regen.sh /path/to/prngs-main
set -euo pipefail
R="${1:?path to prngs-main}/dev/c/families"
cd "$(dirname "$0")"
K="apery_shogi_mt19937_64_evenmask beast2_mersennetwisterfast_69069seed cellkey_record_key_r_runif ecj_mersennetwisterfast_primed gimp_solid_noise_perm_grad_shuffle glib_g_rand_double_52bit glib_g_rand_int_range_2_2_rejection glib_g_rand_mt19937_init_2_0 gnugo_tt800_lcgseed gz_math_rand_mt19937_seedseq melg11213_64 melg1279_64 melg19937_64 melg2281_64 melg4253_64 melg44497_64 melg607_64 minecraft_bedrock_slime_chunk_mt19937 mt19937_php mt19937_seeded mt19937_xor_mask noaa_ncep_iredell_mt19937_69069seed oasislmf_ktools_gulcalc_hashedseed_mt19937 octave_randu53_plus_0p4_mapping ppsspp_sce_mt19937_broken_twist profanity_wintermute_mt19937_64_randomdevice qiskit_aer_mt19937_64_shotseed quackle_mt19937_64_custom_rejection r_unif_index_rounding_bias ruby_mri_int_pair_to_real_exclusive_53bit rusefi_tinymt32_zeromat sdcmicro_recordswap_exp_race sfmt11213 sfmt1279 sfmt132049 sfmt216091 sfmt2281 sfmt4253 sfmt44497 sfmt607 sfmt86243 stim_mt19937_64_versionseed_xor tinymt64 wrk2_tinymt64_degenerate_zeroparams xsadd_saito_matsumoto"
SRC=""; for k in $K; do SRC="$SRC $R/kinds/$k.c"; done
gcc -O2 -w -I"$R" -o /tmp/mtviz_harness harness.c "$R/internal.c" "$R/internal_wave.c" "$R/internal_ext.c" $SRC -lm
/tmp/mtviz_harness > ref.txt
echo "wrote $(wc -l < ref.txt) reference lines"

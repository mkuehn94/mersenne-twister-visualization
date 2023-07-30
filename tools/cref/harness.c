#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include "internal.h"
#include "kind_ops.h"
extern const prng_kind_ops prng_kind_mt19937_php, prng_kind_ppsspp_sce_mt19937_broken_twist, prng_kind_glib_g_rand_mt19937_init_2_0,
 prng_kind_mt19937_seeded, prng_kind_noaa_ncep_iredell_mt19937_69069seed, prng_kind_beast2_mersennetwisterfast_69069seed,
 prng_kind_ecj_mersennetwisterfast_primed, prng_kind_stim_mt19937_64_versionseed_xor, prng_kind_apery_shogi_mt19937_64_evenmask,
 prng_kind_mt19937_xor_mask, prng_kind_rusefi_tinymt32_zeromat;
extern const prng_kind_ops prng_kind_beast2_mersennetwisterfast_69069seed, prng_kind_cellkey_record_key_r_runif, prng_kind_gimp_solid_noise_perm_grad_shuffle, prng_kind_glib_g_rand_double_52bit, prng_kind_glib_g_rand_int_range_2_2_rejection, prng_kind_gnugo_tt800_lcgseed, prng_kind_gz_math_rand_mt19937_seedseq, prng_kind_melg11213_64, prng_kind_melg1279_64, prng_kind_melg19937_64, prng_kind_melg2281_64, prng_kind_melg4253_64, prng_kind_melg44497_64, prng_kind_melg607_64, prng_kind_minecraft_bedrock_slime_chunk_mt19937, prng_kind_mt19937_seeded, prng_kind_oasislmf_ktools_gulcalc_hashedseed_mt19937, prng_kind_octave_randu53_plus_0p4_mapping, prng_kind_profanity_wintermute_mt19937_64_randomdevice, prng_kind_qiskit_aer_mt19937_64_shotseed, prng_kind_quackle_mt19937_64_custom_rejection, prng_kind_r_unif_index_rounding_bias, prng_kind_ruby_mri_int_pair_to_real_exclusive_53bit, prng_kind_sdcmicro_recordswap_exp_race, prng_kind_sfmt11213, prng_kind_sfmt1279, prng_kind_sfmt132049, prng_kind_sfmt216091, prng_kind_sfmt2281, prng_kind_sfmt4253, prng_kind_sfmt44497, prng_kind_sfmt607, prng_kind_sfmt86243, prng_kind_tinymt64, prng_kind_wrk2_tinymt64_degenerate_zeroparams, prng_kind_xsadd_saito_matsumoto;
static uint8_t st[1<<22];
static void kind(const char*name,const prng_kind_ops*k,uint64_t p0,uint64_t p1,uint64_t p2,uint64_t seed,int u64){
  uint64_t p[8]={p0,p1,p2,0,0,0,0,0}; memset(st,0,sizeof st); k->seed(st,seed,p);
  printf("%s %llu:",name,(unsigned long long)seed);
  for(int i=0;i<6;i++){ if(u64) printf(" %llu",(unsigned long long)k->next_u64(st,p)); else printf(" %u",k->next_u32(st,p)); }
  // also 1000th
  for(int i=6;i<999;i++){ if(u64) k->next_u64(st,p); else k->next_u32(st,p);}
  if(u64) printf(" | %llu\n",(unsigned long long)k->next_u64(st,p)); else printf(" | %u\n",k->next_u32(st,p));
}
int main(){
  uint64_t seeds[]={5489,42,0,1234,(1ull<<40)+5};
  for(int si=0;si<5;si++){ uint64_t s=seeds[si];
    { mt_t m; mt_seed(&m,s); printf("mt19937 %llu:",(unsigned long long)s); for(int i=0;i<6;i++) printf(" %u",mt_next(&m)); for(int i=6;i<999;i++) mt_next(&m); printf(" | %u\n",mt_next(&m)); }
    { mt64_t m; mt64_seed(&m,s); printf("mt64 %llu:",(unsigned long long)s); for(int i=0;i<6;i++) printf(" %llu",(unsigned long long)mt64_next(&m)); for(int i=6;i<999;i++) mt64_next(&m); printf(" | %llu\n",(unsigned long long)mt64_next(&m)); }
    { tinymt_t m; tinymt_seed(&m,s); printf("tinymt32 %llu:",(unsigned long long)s); for(int i=0;i<6;i++) printf(" %u",tinymt_next(&m)); for(int i=6;i<999;i++) tinymt_next(&m); printf(" | %u\n",tinymt_next(&m)); }
    { sfmt_t m; sfmt_seed(&m,s); printf("sfmt %llu:",(unsigned long long)s); for(int i=0;i<6;i++) printf(" %u",sfmt_next(&m)); for(int i=6;i<999;i++) sfmt_next(&m); printf(" | %u\n",sfmt_next(&m)); }
    { mt11213_t m; mt11213_seed(&m,s); printf("mt11213 %llu:",(unsigned long long)s); for(int i=0;i<6;i++) printf(" %u",mt11213_next(&m)); for(int i=6;i<999;i++) mt11213_next(&m); printf(" | %u\n",mt11213_next(&m)); }
    { tt800_t m; tt800_seed(&m,s); printf("tt800 %llu:",(unsigned long long)s); for(int i=0;i<6;i++) printf(" %u",tt800_next(&m)); for(int i=6;i<999;i++) tt800_next(&m); printf(" | %u\n",tt800_next(&m)); }
    kind("php_cokus_broken_shift",&prng_kind_mt19937_php,1,1,1,s,0);
    kind("php_wagner_broken_shift",&prng_kind_mt19937_php,1,0,1,s,0);
    kind("php_wagner_std_shift",&prng_kind_mt19937_php,0,0,1,s,0);
    kind("php_cokus_std_noshift",&prng_kind_mt19937_php,0,1,0,s,0);
    kind("ppsspp",&prng_kind_ppsspp_sce_mt19937_broken_twist,0,0,0,s,0);
    kind("glib20",&prng_kind_glib_g_rand_mt19937_init_2_0,0,0,0,s,0);
    kind("seeded_byarray",&prng_kind_mt19937_seeded,1,0,0,s,0);
    kind("noaa",&prng_kind_noaa_ncep_iredell_mt19937_69069seed,0,0,0,s,0);
    kind("beast2",&prng_kind_beast2_mersennetwisterfast_69069seed,0,0,0,s,0);
    kind("ecj",&prng_kind_ecj_mersennetwisterfast_primed,0,0,0,s,0);
    kind("stim",&prng_kind_stim_mt19937_64_versionseed_xor,0,0,0,s,1);
    kind("apery",&prng_kind_apery_shogi_mt19937_64_evenmask,0,0,0,s,1);
    kind("mybb_mask1",&prng_kind_mt19937_xor_mask,1,0,0,s,0);
    kind("tinymt64",&prng_kind_tinymt64,4194639680ULL,4291887092ULL,6399667842752446396ULL,s,1);
    kind("wrk2",&prng_kind_wrk2_tinymt64_degenerate_zeroparams,0,0,0,s,1);
    kind("melg607",&prng_kind_melg607_64,0,0,0,s,1);
    kind("melg1279",&prng_kind_melg1279_64,0,0,0,s,1);
    kind("melg2281",&prng_kind_melg2281_64,0,0,0,s,1);
    kind("melg4253",&prng_kind_melg4253_64,0,0,0,s,1);
    kind("melg11213",&prng_kind_melg11213_64,0,0,0,s,1);
    kind("melg19937",&prng_kind_melg19937_64,0,0,0,s,1);
    kind("melg44497",&prng_kind_melg44497_64,0,0,0,s,1);
    kind("sfmt607",&prng_kind_sfmt607,0,0,0,s,0);
    kind("sfmt1279",&prng_kind_sfmt1279,0,0,0,s,0);
    kind("sfmt2281",&prng_kind_sfmt2281,0,0,0,s,0);
    kind("sfmt4253",&prng_kind_sfmt4253,0,0,0,s,0);
    kind("sfmt11213",&prng_kind_sfmt11213,0,0,0,s,0);
    kind("sfmt44497",&prng_kind_sfmt44497,0,0,0,s,0);
    kind("sfmt86243",&prng_kind_sfmt86243,0,0,0,s,0);
    kind("sfmt132049",&prng_kind_sfmt132049,0,0,0,s,0);
    kind("sfmt216091",&prng_kind_sfmt216091,0,0,0,s,0);
    kind("xsadd",&prng_kind_xsadd_saito_matsumoto,0,0,0,s,0);
    kind("gnugo",&prng_kind_gnugo_tt800_lcgseed,0,0,0,s,0);
    kind("gz",&prng_kind_gz_math_rand_mt19937_seedseq,0,0,0,s,0);
    kind("minecraft",&prng_kind_minecraft_bedrock_slime_chunk_mt19937,3,5,0,s,0);
    kind("oasis07",&prng_kind_oasislmf_ktools_gulcalc_hashedseed_mt19937,0,7,0,s,0);
    kind("oasis11",&prng_kind_oasislmf_ktools_gulcalc_hashedseed_mt19937,1,1,0,s,0);
    kind("octave",&prng_kind_octave_randu53_plus_0p4_mapping,0,0,0,s,0);
    kind("ruby",&prng_kind_ruby_mri_int_pair_to_real_exclusive_53bit,0,0,0,s,0);
    kind("r_index",&prng_kind_r_unif_index_rounding_bias,1000000,0,0,s,0);
    kind("cellkey",&prng_kind_cellkey_record_key_r_runif,8,0,0,s,0);
    kind("glib_double",&prng_kind_glib_g_rand_double_52bit,0,0,0,s,0);
    kind("glib_range",&prng_kind_glib_g_rand_int_range_2_2_rejection,1000000,0,0,s,0);
    kind("gimp",&prng_kind_gimp_solid_noise_perm_grad_shuffle,0,0,0,s,0);
    kind("sdcmicro",&prng_kind_sdcmicro_recordswap_exp_race,16,0,0,s,0);
    kind("beast2_m1",&prng_kind_beast2_mersennetwisterfast_69069seed,1,0,0,s,0);
    kind("profanity",&prng_kind_profanity_wintermute_mt19937_64_randomdevice,0,0,0,s,1);
    kind("qiskit",&prng_kind_qiskit_aer_mt19937_64_shotseed,0,0,0,s,1);
    kind("quackle",&prng_kind_quackle_mt19937_64_custom_rejection,0,99,0,s,0);
    kind("seeded_genrand",&prng_kind_mt19937_seeded,0,0,0,s,0);
    kind("seeded_cokus2",&prng_kind_mt19937_seeded,2,0,0,s,0);
    kind("rusefi",&prng_kind_rusefi_tinymt32_zeromat,s,0,0,s,0);
  }
}

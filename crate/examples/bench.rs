use mtviz::gf2::*;
use mtviz::mt::MtConfig;
fn main(){
  let t=std::time::Instant::now();
  let (f,l)=char_poly(&MtConfig::mt19937(),5489);
  println!("BM deg {} in {:?}, weight {}", l, t.elapsed(), weight(&f));
  let mut j=IrrJob::new(&f,0);
  let t=std::time::Instant::now();
  j.step(500);
  let e=t.elapsed(); println!("500 squarings {:?} => full {:?}", e, e*19937/500);
  let t=std::time::Instant::now();
  let mut k=KdistJob::new(&MtConfig::new(32,40,20,1,0x9908b0df,11,0xffffffff,7,0x9d2c5680,15,0xefc60000,18,1812433253));
  println!("kdist build 1279 {:?}", t.elapsed());
  while !k.step(){}
  println!("kdist 1279 {:?} {:?}", t.elapsed(), k.k);
  // top terms of char poly
  let d=degree(&f).unwrap(); let mut ex=vec![]; for i in (0..=d).rev(){ if get_bit(&f,i){ex.push(i);} } println!("exps top {:?}", &ex[..10]); println!("exps bottom {:?}", &ex[ex.len()-10..]);
}

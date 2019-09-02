use rand::seq::SliceRandom;
use rand::thread_rng;

pub mod prep_db;
pub mod time;

#[inline]
pub fn take_one<'a, T: ?Sized>(list: &[&'a T]) -> &'a T {
    let mut rng = thread_rng();
    list.choose(&mut rng).expect("take_one got empty list")
}

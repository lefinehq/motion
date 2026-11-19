use super::*;
use crate::FontSet;

#[test]
fn inter_is_a_sans_of_normal_width() {
    let set = FontSet::builtin();
    let face = set.named("InterVariable").expect("Inter ships built in");
    let metrics = face.metrics();
    assert_eq!(metrics.class, Class::Sans);
    assert_eq!(metrics.stretch, 5);
    // Inter's x-height is about 0.55 em, its average advance about 0.6.
    assert!((0.5..0.6).contains(&metrics.x_height), "{metrics:?}");
    assert!((0.45..0.7).contains(&metrics.width), "{metrics:?}");
}

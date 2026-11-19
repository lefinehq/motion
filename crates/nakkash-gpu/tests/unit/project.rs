use super::*;

#[test]
fn a_chain_halves_down_to_a_px() {
    assert_eq!(chain_sizes(1, 1), vec![(1, 1)]);
    assert_eq!(chain_sizes(5, 3), vec![(5, 3), (3, 2), (2, 1), (1, 1)]);
    assert_eq!(chain_sizes(4, 4), vec![(4, 4), (2, 2), (1, 1)]);
}

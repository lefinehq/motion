use super::*;

#[test]
fn an_amount_on_the_cpu_multiplies_its_shares_in_order() {
    let (a, b) = ([0.5, 0.25], [0.5, 2.0]);
    let amount = Amount::constant(0.8)
        .and(Share::Values(&a))
        .and_values(Some(&b))
        .and_values(None);
    assert!(amount.on_cpu());
    assert_eq!(amount.at(0), 0.8 * 0.5 * 0.5);
    assert_eq!(amount.at(1), 0.8 * 0.25 * 2.0);
}

#[test]
fn surfaces_without_an_accelerator_work_on_the_cpu() {
    let mut canvas = Surface::Cpu(Pixels::transparent(2, 1));
    let mut red = Pixels::transparent(2, 1);
    red.data_mut()[0] = [1.0, 0.0, 0.0, 1.0];
    let red = Surface::Cpu(red);
    canvas
        .lay(&red, BlendMode::Normal, &Amount::constant(0.5), None)
        .expect("laid");
    canvas.cut(&[0.5, 1.0], None).expect("cut");
    let pixels = canvas.into_pixels(None).expect("on the cpu");
    assert_eq!(pixels.data()[0], [0.25, 0.0, 0.0, 0.25]);
    assert_eq!(pixels.data()[1], [0.0; 4]);
}

use super::*;
#[test]
fn scaled_rounding_printing_and_arithmetic_boundaries() {
    assert_eq!(round_decimals(&[5]), 32768);
    assert_eq!(
        round_decimals(&[0, 0, 0, 0, 0, 7, 6, 2, 9, 3, 9, 4, 5, 3, 1, 2, 5]),
        1
    );
    for (value, text) in [
        (0, "0.0"),
        (1, "0.00002"),
        (-1, "-0.00002"),
        (65535, "0.99998"),
        (65536, "1.0"),
        (1073741823, "16383.99998"),
    ] {
        let mut bytes = Vec::new();
        print_scaled(Scaled(value), |b| bytes.push(b));
        assert_eq!(bytes, text.as_bytes());
    }
    assert_eq!(Scaled::checked(1073741824), Err(Error::Overflow));
    assert_eq!(Scaled(-7).divide(3), Ok(Scaled(-2)));
    assert_eq!(Scaled::MAX.multiply(2), Err(Error::Overflow));
    assert_eq!(Scaled(i32::MAX).advance(Scaled(1)), Scaled(i32::MIN));
    assert_eq!(Scaled(2).divide(0), Err(Error::Overflow));
}
#[test]
fn glue_addition_follows_operand_order_and_keeps_zero_component_orders() {
    let old = Glue {
        width: Scaled(1),
        stretch: Scaled(10),
        stretch_order: GlueOrder::Fil,
        ..Glue::default()
    };
    let operand = Glue {
        stretch: Scaled(-10),
        stretch_order: GlueOrder::Fil,
        ..Glue::default()
    };
    let sum = old.advance(operand);
    assert_eq!(sum.width, Scaled(1));
    assert_eq!(sum.stretch, Scaled(0));
    assert_eq!(sum.stretch_order, GlueOrder::Fil);
    let high_zero = Glue {
        width: Scaled(1),
        stretch_order: GlueOrder::Fill,
        ..Glue::default()
    };
    assert_eq!(high_zero.advance(old).stretch_order, GlueOrder::Fil);
    assert_eq!(old.arithmetic(0, false), Ok(Glue::default()));
    let mut wide = old;
    wide.shrink = Scaled::MAX;
    assert_eq!(wide.arithmetic(2, false), Err(Error::Overflow));
}
#[test]
fn badness_uses_tex_integer_thresholds() {
    for (t, s, want) in [
        (0, 0, 0),
        (1, 0, 10000),
        (65536, 65536, 100),
        (32768, 65536, 12),
        (131072, 65536, 800),
        (10000000, 100, 10000),
        (10000000, 2000000, 10000),
    ] {
        assert_eq!(badness(Scaled(t), Scaled(s)), want);
    }
}

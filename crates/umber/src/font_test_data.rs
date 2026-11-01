//! Small controlled TFM fixtures; metrics vary by character, with real TFM tables.
pub(crate) fn tfm(program: &[[u8; 4]], tags: &[(u8, u8, u8)]) -> Vec<u8> {
    tfm_range(0, 127, program, tags)
}
pub(crate) fn tfm_range(
    first: u8,
    last: u8,
    program: &[[u8; 4]],
    tags: &[(u8, u8, u8)],
) -> Vec<u8> {
    let lh = 2;
    let count = usize::from(last) + 1 - usize::from(first);
    let nw = 4;
    let nh = 2;
    let nd = 2;
    let ni = 2;
    let nk = 1;
    let np = 7;
    let lf = 6 + lh + count + nw + nh + nd + ni + program.len() + nk + np;
    let mut b = Vec::new();
    for n in [
        lf,
        lh,
        first as usize,
        last as usize,
        nw,
        nh,
        nd,
        ni,
        program.len(),
        nk,
        0,
        np,
    ] {
        b.extend_from_slice(&(n as u16).to_be_bytes());
    }
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&(10i32 << 20).to_be_bytes());
    for c in first..=last {
        let (tag, rem) = tags
            .iter()
            .find(|x| x.0 == c)
            .map_or((0, 0), |x| (x.1, x.2));
        let width = if c == 0 { 0 } else { 1 + c % 3 };
        b.extend_from_slice(&[width, 0x11, 4 | tag, rem]);
    }
    for v in [0, 524288, 786432, 262144, 0, 734003, 0, 209715, 0, 104858] {
        b.extend_from_slice(&i32::to_be_bytes(v));
    }
    for row in program {
        b.extend_from_slice(row);
    }
    b.extend_from_slice(&(-104858i32).to_be_bytes());
    for v in [104858, 349525, 174763, 116508, 471859, 1048576, 116508] {
        b.extend_from_slice(&i32::to_be_bytes(v));
    }
    b
}
pub(crate) fn basic() -> Vec<u8> {
    tfm(
        &[
            [0, b'i', 0, 12],
            [0, b'f', 0, 11],
            [128, b'l', 0, 13],
            [128, b'i', 0, 14],
            [128, b'V', 128, 0],
        ],
        &[(b'f', 1, 0), (11, 1, 3), (b'A', 1, 4)],
    )
}

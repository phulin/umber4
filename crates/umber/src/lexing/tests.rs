use super::hash::{NameHash, Word};
use super::*;

fn tables() -> [Table; 4] {
    [
        Table::initex(),
        Table::plain(),
        Table::package(),
        Table::expl3(),
    ]
}

fn tokens(lexer: Lexer, source: &Source, table: &Table, session: &mut Session) -> Vec<u32> {
    let mut out = Vec::new();
    lex(lexer, source, table, session, &mut out);
    out
}

/// Every available prototype returns the current reader's tokens and interns
/// the same names in the same order, so identities agree.
fn assert_matches_current(text: &[u8], table: &Table) {
    let source = Source::new(text);
    let mut current = Session::new(Lexer::Current);
    let expected = tokens(Lexer::Current, &source, table, &mut current);
    for lexer in Lexer::ALL[1..]
        .iter()
        .copied()
        .filter(|lexer| lexer.available())
    {
        let mut session = Session::new(lexer);
        let actual = tokens(lexer, &source, table, &mut session);
        assert!(
            actual == expected && session.names() == current.names(),
            "{} differs on {:?} with endline {}:\n{actual:x?}\n{expected:x?}",
            lexer.name(),
            String::from_utf8_lossy(text),
            table.endline,
        );
        assert_eq!(
            tokens(lexer, &source, table, &mut session),
            expected,
            "warm {}",
            lexer.name()
        );
        assert_eq!(session.names(), current.names());
    }
}

#[test]
fn fixtures_match_the_current_reader() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mvp");
    let mut checked = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "tex") {
            let text = std::fs::read(&path).unwrap();
            for table in tables() {
                assert_matches_current(&text, &table);
            }
            checked += 1;
        }
    }
    assert!(checked > 10);
}

#[test]
fn line_edges_match_the_current_reader() {
    let cases: &[&[u8]] = &[
        b"",
        b"\n",
        b"\r\n\r\n",
        b"a",
        b"a\n",
        b"a   \nb",
        b"  \n",
        b" \0\n",
        b"\t\n\tx",
        b"\\",
        b"\\\n",
        b"\\ ",
        b"\\  \nx",
        b"\\a",
        b"\\ab\\c d",
        b"\\abc",
        b"\\foo  bar \\baz{x}",
        b"x^^41y^^5a^^^",
        b"\\fo^^6fo bar",
        b"\\foo^^",
        b"\\foo^^:",
        b"\\^^5cfoo",
        b"^^",
        b"^^\n^",
        b"a^^ ",
        b"a^^z^^\xff",
        b"%comment\nx%\n\\x%",
        b"a\r\nb\rc\n\nd",
        b"\x7f a \x7f",
        b"~a~ ~\x0c",
        b"word word  word   word\tword",
        b"a b c d e f g h i j k l m n o p q r s t u v w x y z a b c d e f g h i j k l m n o p q r s t",
        b"\\averyveryveryverylongcontrolwordthatcrossesavectorboundaryandmore{x} \\b",
        b"@\\@ifnextchar[\\@tempa:_\\l__foo_bar:Nn~x \"|",
    ];
    for &text in cases {
        for mut table in tables() {
            for endline in [13, -1, 32, 9, i32::from(b'a'), 92, 94, 37, 126, 0, 256] {
                table.endline = endline;
                assert_matches_current(text, &table);
            }
        }
    }
}

/// A small xorshift generator keeps the comparison deterministic without a
/// dependency.
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn random_text_and_tables_match_the_current_reader() {
    const ALPHABET: &[u8] = b"\\\\\\\\aaabbbzZ@:_      \t\n\n\r%^^^^{}#~0129af\x00\x7f\xc3\xa9\x0c";
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    for _ in 0..4000 {
        let longest = if random.below(8) == 0 { 400 } else { 60 };
        let len = random.below(longest);
        let text: Vec<u8> = (0..len)
            .map(|_| ALPHABET[random.below(ALPHABET.len())])
            .collect();
        let mut table = tables()[random.below(4)].clone();
        for _ in 0..random.below(4) {
            table.cats[ALPHABET[random.below(ALPHABET.len())] as usize] = random.below(16) as u8;
        }
        table.endline = [13, 13, 13, -1, 32, 97, 92, 94, 37, 300][random.below(10)];
        assert_matches_current(&text, &table);
    }
}

#[test]
fn long_runs_cross_mask_words() {
    let mut random = Random(0x2545_f491_4f6c_dd1d);
    for _ in 0..300 {
        let mut text = Vec::new();
        for _ in 0..random.below(80) {
            match random.below(12) {
                0 => text.push(b'\\'),
                1 => text.extend_from_slice(b"  "),
                2 => text.push(b'\n'),
                3 => text.extend_from_slice(b"{}"),
                _ => {}
            }
            let word = random.below(90) + 1;
            text.extend((0..word).map(|i| b"abcdefghij,.()0123{}#$&_~"[(i * 7 + word) % 25]));
            text.push(b' ');
        }
        for table in tables() {
            assert_matches_current(&text, &table);
        }
    }
}

#[test]
fn word_hash_reads_names_in_place() {
    let mut random = Random(0x1234_5678_9abc_def1);
    let mut text = vec![0; 128 + PAD];
    for byte in &mut text {
        *byte = random.next() as u8;
    }
    for start in 0..8 {
        for len in 2..=80 {
            let name = &text[start..start + len];
            assert_eq!(Word::padded(&text, start, len), Word::exact(name), "{len}");
        }
    }
}

//! Fresh-process native command-line format construction and loading.
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "umber-format-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
    fn run(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
        Command::new(env!("CARGO_BIN_EXE_umber"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn success(&self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Vec<u8> {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    }
    fn no_temporary_files(&self) {
        assert!(
            std::fs::read_dir(&self.0).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp-"))
        );
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn mapping(name: &str, path: &Path) -> String {
    format!("{name}={}", path.display())
}

#[test]
fn build_load_fresh_calendar_hooks_inputs_and_legacy_cli() {
    let d = Directory::new();
    let setup = d.file("setup.tex", br"\input bootstrap \dump");
    let bootstrap=d.file("bootstrap.tex",br"\catcode123=1\catcode125=2\def\saved{YES}\edef\built{\the\year}\count0=7\everyjob{\global\advance\count0by1\message{BOOT:\the\count0/\the\year/\the\time}}\endinput");
    let image = d.0.join("plain.ufmt");
    d.success([
        "build-format",
        setup.to_str().unwrap(),
        "-o",
        image.to_str().unwrap(),
        "--input",
        &mapping("bootstrap", &bootstrap),
        "--calendar",
        "15,2,3,1900",
    ]);
    let bytes = std::fs::read(&image).unwrap();
    assert!(bytes.starts_with(b"UMBERFMT"));
    // A fresh process needs neither the setup file nor its resolver mapping.
    std::fs::remove_file(setup).unwrap();
    std::fs::remove_file(bootstrap).unwrap();
    let job = d.file(
        "job.tex",
        br"\message{JOB:\saved/\built/\the\year/\the\count0}\end",
    );
    assert_eq!(
        d.success([job.to_str().unwrap(), "--format", image.to_str().unwrap()]),
        b"BOOT:8/1970/0\nJOB:YES/1900/1970/8\n"
    );
    assert_eq!(
        d.success([
            job.to_str().unwrap(),
            "--format",
            image.to_str().unwrap(),
            "--calendar",
            "123,2,3,2026"
        ]),
        b"BOOT:8/2026/123\nJOB:YES/1900/2026/8\n"
    );
    let legacy = d.file(
        "legacy.tex",
        br"\catcode123=1\catcode125=2\message{LEGACY:\the\year}\end",
    );
    assert_eq!(d.success([legacy.to_str().unwrap()]), b"LEGACY:1970\n");
    d.no_temporary_files();
}

fn tfm() -> Vec<u8> {
    // A one-character, half-em font with seven standard parameters.
    let mut bytes = Vec::new();
    for word in [21u16, 2, 97, 97, 2, 1, 1, 1, 0, 0, 0, 7] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&(10i32 << 20).to_be_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0]);
    for word in [0i32, 524288, 0, 0, 0, 0, 349525, 0, 0, 0, 1048576, 0] {
        bytes.extend_from_slice(&word.to_be_bytes());
    }
    bytes
}
#[test]
fn loaded_fonts_are_self_contained_and_hyphenation_startup_is_frozen() {
    let d = Directory::new();
    let font = d.file("test.tfm", &tfm());
    let setup=d.file("setup.tex",br"\catcode123=1\catcode125=2\font\f=test\fontdimen2\f=2.5pt\patterns{a1a}\hyphenation{a-a}\dump");
    let image = d.0.join("font.ufmt");
    d.success([
        "build-format",
        setup.to_str().unwrap(),
        "-o",
        image.to_str().unwrap(),
        "--tfm",
        &mapping("test", &font),
    ]);
    let saved = d.0.join("saved.tfm");
    std::fs::rename(&font, &saved).unwrap();
    let job=d.file("font-job.tex",br"\font\g=test at7pt\hyphenation{aa-a}\message{FONT:\the\fontdimen2\f/\the\fontdimen6\g}\end");
    let expected = b"FONT:2.5pt/7.0pt\n";
    assert_eq!(
        d.success([job.to_str().unwrap(), "--format", image.to_str().unwrap()]),
        expected
    );
    assert_eq!(
        d.success([
            job.to_str().unwrap(),
            "--format",
            image.to_str().unwrap(),
            "--tfm",
            &mapping("test.tfm", &saved)
        ]),
        expected
    );
    let mut changed = tfm();
    changed.push(0);
    let changed = d.file("changed.tfm", &changed);
    let error = d.run([
        job.to_str().unwrap(),
        "--format",
        image.to_str().unwrap(),
        "--tfm",
        &mapping("test", &changed),
    ]);
    assert!(!error.status.success());
    assert!(error.stdout.is_empty());
    assert!(String::from_utf8_lossy(&error.stderr).contains("Resource"));
    let job = d.file("patterns.tex", br"\patterns{a1a}\end");
    let error = d.run([job.to_str().unwrap(), "--format", image.to_str().unwrap()]);
    assert!(!error.status.success());
    assert!(String::from_utf8_lossy(&error.stderr).contains("Syntax"));
}

#[test]
fn failed_builds_and_loads_do_not_publish_partial_artifacts_or_job_output() {
    let d = Directory::new();
    let image = d.file("keep.ufmt", b"old artifact");
    let setup = d.file("bad.tex", br"\undefined\dump");
    let error = d.run([
        "build-format",
        setup.to_str().unwrap(),
        "-o",
        image.to_str().unwrap(),
    ]);
    assert!(!error.status.success());
    assert!(error.stdout.is_empty());
    assert_eq!(std::fs::read(&image).unwrap(), b"old artifact");
    let job = d.file("job.tex", br"\message{MUSTNOTRUN}\end");
    let error = d.run([job.to_str().unwrap(), "--format", image.to_str().unwrap()]);
    assert!(!error.status.success());
    assert!(error.stdout.is_empty());
    assert!(String::from_utf8_lossy(&error.stderr).contains("Format"));
    let setup = d.file("good.tex", br"\dump");
    let occupied = d.0.join("directory");
    std::fs::create_dir(&occupied).unwrap();
    let error = d.run([
        "build-format",
        setup.to_str().unwrap(),
        "-o",
        occupied.to_str().unwrap(),
    ]);
    assert!(!error.status.success());
    assert!(occupied.is_dir());
    d.no_temporary_files();
    d.success([
        "build-format",
        setup.to_str().unwrap(),
        "-o",
        image.to_str().unwrap(),
    ]);
    assert!(std::fs::read(&image).unwrap().starts_with(b"UMBERFMT"));
    let invalid = d.run([job.to_str().unwrap(), "--calendar", "1,2,3"]);
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
}

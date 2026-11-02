//! Prepared document endpoints; discovery and retained-output checks are separate.
use super::super::{equal, equal_page};
use super::{Bundle, RunResult, Settings, complete};
use std::{hint::black_box, io, time::Instant};
use umber::{Checkpoint, Engine, Outcome, Progress};

struct Point<'a> {
    label: &'static str,
    requested: usize,
    pages: usize,
    checkpoint: Checkpoint<'a>,
    prefix: Outcome,
}
struct Prepared<'a> {
    engine: Engine<'a>,
    reference: Outcome,
    setup_ns: u128,
    advance_ns: u128,
    outcome_ns: u128,
}
fn prefix_equal(prefix: &Outcome, reference: &Outcome) {
    assert!(prefix.diagnostics.len() <= reference.diagnostics.len());
    assert_eq!(
        prefix.diagnostics,
        reference.diagnostics[..prefix.diagnostics.len()]
    );
    assert!(prefix.pages.len() <= reference.pages.len());
    for i in 0..prefix.pages.len() {
        equal_page(
            prefix.pages.get(i).unwrap(),
            reference.pages.get(i).unwrap(),
        );
    }
}
fn point<'a>(
    engine: &Engine<'a>,
    label: &'static str,
    requested: usize,
    prefix: Outcome,
) -> Point<'a> {
    assert!(engine.can_checkpoint());
    Point {
        label,
        requested,
        pages: prefix.pages.len(),
        checkpoint: engine.checkpoint().unwrap(),
        prefix,
    }
}
fn prepare<'a>(
    bundle: &'a Bundle,
    settings: &Settings,
    points: &mut Vec<Point<'a>>,
) -> RunResult<Prepared<'a>> {
    let start = Instant::now();
    let mut reference_engine = bundle.make()?;
    let setup_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    complete(&mut reference_engine)?;
    let advance_ns = start.elapsed().as_nanos();
    let start = Instant::now();
    let reference = reference_engine.outcome();
    let outcome_ns = start.elapsed().as_nanos();
    drop(reference_engine);
    let total = reference.pages.len();
    if total == 0 {
        return Err(io::Error::other("page-progress preparation needs nonempty output").into());
    }
    let targets = [
        ("early", total.div_ceil(10)),
        ("middle", total.div_ceil(2)),
        ("late", total - total / 10),
    ];
    let mut engine = bundle.make()?;
    if bundle.format.is_some() {
        points.push(point(&engine, "ready", 0, engine.outcome()));
    }
    let mut selected = 0;
    let mut ready = settings.ready.is_none();
    loop {
        let progress = engine.advance(1)?;
        let current = engine.outcome();
        if !ready
            && current
                .diagnostics
                .iter()
                .any(|d| Some(d.bytes()) == settings.ready.as_deref())
        {
            if !current.pages.is_empty() {
                return Err(io::Error::other("READY marker must precede document pages").into());
            }
            points.push(point(&engine, "ready", 0, engine.outcome()));
            ready = true;
        }
        while selected < targets.len() && current.pages.len() >= targets[selected].1 {
            let (label, target) = targets[selected];
            points.push(point(&engine, label, target, engine.outcome()));
            selected += 1;
        }
        if progress == Progress::Complete {
            equal(&reference, &current);
            break;
        }
    }
    if !ready || selected != targets.len() {
        return Err(io::Error::other("requested checkpoint boundary not observed").into());
    }
    Ok(Prepared {
        engine,
        reference,
        setup_ns,
        advance_ns,
        outcome_ns,
    })
}
fn check_old(points: &[Point<'_>], old: &[Outcome], reference: &Outcome) {
    for point in points {
        prefix_equal(&point.prefix, reference);
    }
    for outcome in old {
        equal(outcome, reference);
    }
}
fn validate<'a>(
    prepared: &mut Prepared<'a>,
    points: &[Point<'a>],
    old: &mut Vec<Outcome>,
) -> RunResult<()> {
    for point in points {
        prepared.engine.restore(&point.checkpoint)?;
        complete(&mut prepared.engine)?;
        let outcome = prepared.engine.outcome();
        equal(&prepared.reference, &outcome);
        check_old(points, old, &prepared.reference);
        if old.len() < 3 {
            old.push(outcome);
        }
    }
    old.clear();
    prepared
        .engine
        .restore(&points.last().unwrap().checkpoint)?;
    complete(&mut prepared.engine)?;
    equal(&prepared.reference, &prepared.engine.outcome());
    check_old(points, old, &prepared.reference);
    Ok(())
}
struct Sample {
    label: &'static str,
    restore_completed: u128,
    stable_restore: f64,
    capture: f64,
    release: f64,
    suffix: u128,
    outcome: u128,
    equality: u128,
    outcome_release: u128,
}
pub(super) fn run(bundle: &Bundle, settings: &Settings) -> RunResult<()> {
    let mut points = Vec::with_capacity(4);
    let mut old = Vec::with_capacity(3);
    let mut captures = Vec::with_capacity(settings.iterations);
    let mut samples = Vec::with_capacity(
        settings
            .repeats
            .checked_mul(4)
            .ok_or_else(|| io::Error::other("too many samples"))?,
    );
    let mut prepared = prepare(bundle, settings, &mut points)?;
    validate(&mut prepared, &points, &mut old)?;
    for point in &points {
        for repeat in 0..=settings.repeats {
            let start = Instant::now();
            prepared.engine.restore(&point.checkpoint)?;
            let restore_completed = start.elapsed().as_nanos();
            let start = Instant::now();
            for _ in 0..settings.iterations {
                prepared.engine.restore(black_box(&point.checkpoint))?;
            }
            let stable_restore = start.elapsed().as_nanos() as f64 / settings.iterations as f64;
            let start = Instant::now();
            for _ in 0..settings.iterations {
                captures.push(black_box(prepared.engine.checkpoint()?));
            }
            let capture = start.elapsed().as_nanos() as f64 / settings.iterations as f64;
            let start = Instant::now();
            captures.clear();
            let release = start.elapsed().as_nanos() as f64 / settings.iterations as f64;
            let start = Instant::now();
            complete(&mut prepared.engine)?;
            let suffix = start.elapsed().as_nanos();
            let start = Instant::now();
            let outcome = prepared.engine.outcome();
            let outcome_ns = start.elapsed().as_nanos();
            let start = Instant::now();
            equal(&prepared.reference, &outcome);
            check_old(&points, &old, &prepared.reference);
            let equality = start.elapsed().as_nanos();
            let start = Instant::now();
            drop(outcome);
            let outcome_release = start.elapsed().as_nanos();
            if repeat != 0 {
                samples.push(Sample {
                    label: point.label,
                    restore_completed,
                    stable_restore,
                    capture,
                    release,
                    suffix,
                    outcome: outcome_ns,
                    equality,
                    outcome_release,
                });
            }
        }
    }
    print!(
        "{{\"pages\":{},\"clock\":\"wall\",\"repeats\":{},\"warmups\":1,\"iterations\":{},\"reference\":{{\"samples\":1,\"engine_setup_ns\":{},\"advance_ns\":{},\"outcome_ns\":{}}},\"checkpoints\":[",
        prepared.reference.pages.len(),
        settings.repeats,
        settings.iterations,
        prepared.setup_ns,
        prepared.advance_ns,
        prepared.outcome_ns
    );
    for (i, point) in points.iter().enumerate() {
        if i != 0 {
            print!(",");
        }
        print!(
            "{{\"label\":\"{}\",\"requested_pages\":{},\"actual_pages\":{}}}",
            point.label, point.requested, point.pages
        );
    }
    print!("],\"samples\":[");
    for (i, s) in samples.iter().enumerate() {
        if i != 0 {
            print!(",");
        }
        print!(
            "{{\"checkpoint\":\"{}\",\"restore_completed_ns\":{},\"stable_restore_ns\":{},\"capture_retained_ns\":{},\"release_retained_ns\":{},\"suffix_advance_ns\":{},\"outcome_ns\":{},\"equality_ns\":{},\"outcome_release_ns\":{}}}",
            s.label,
            s.restore_completed,
            s.stable_restore,
            s.capture,
            s.release,
            s.suffix,
            s.outcome,
            s.equality,
            s.outcome_release
        );
    }
    println!("],\"validation\":\"complete output and retained prefixes/results equal\"}}");
    Ok(())
}

pub(super) fn alloc(
    bundle: &Bundle,
    settings: &Settings,
    mark: &mut impl FnMut(&'static str),
) -> RunResult<()> {
    // These helper backings outlive counting. Only their tracked semantic owners
    // are cleared before the last mark, avoiding frees of uncounted allocations.
    let mut points = Vec::with_capacity(4);
    let mut captures = Vec::with_capacity(settings.iterations);
    let mut old = Vec::with_capacity(3);
    mark("begin");
    let mut prepared = prepare(bundle, settings, &mut points)?;
    validate(&mut prepared, &points, &mut old)?;
    mark("prepared_with_reference_and_prefixes");
    let checkpoint = &points.last().unwrap().checkpoint;
    prepared.engine.restore(checkpoint)?;
    mark("completed_state_restored_to_late");
    for _ in 0..settings.iterations {
        captures.push(prepared.engine.checkpoint()?);
    }
    mark("captures_retained");
    captures.clear();
    mark("captures_released");
    for _ in 0..settings.iterations {
        prepared.engine.restore(checkpoint)?;
    }
    mark("stable_restores");
    complete(&mut prepared.engine)?;
    mark("suffix_advanced");
    let outcome = prepared.engine.outcome();
    mark("suffix_outcome_acquired");
    equal(&prepared.reference, &outcome);
    check_old(&points, &old, &prepared.reference);
    mark("suffix_equality_checked");
    drop(outcome);
    mark("suffix_outcome_released");
    for (count, label) in [
        (1, "replay_1_with_transient_outcomes"),
        (5, "replay_5_with_transient_outcomes"),
        (20, "replay_20_with_transient_outcomes"),
    ] {
        for _ in 0..count {
            prepared.engine.restore(checkpoint)?;
            complete(&mut prepared.engine)?;
            equal(&prepared.reference, &prepared.engine.outcome());
        }
        check_old(&points, &old, &prepared.reference);
        mark(label);
    }
    for _ in 0..3 {
        prepared.engine.restore(checkpoint)?;
        complete(&mut prepared.engine)?;
        old.push(prepared.engine.outcome());
    }
    mark("three_suffix_outcomes_retained");
    check_old(&points, &old, &prepared.reference);
    mark("retained_suffixes_checked");
    old.clear();
    mark("retained_suffixes_released");
    prepared.engine.restore(checkpoint)?;
    complete(&mut prepared.engine)?;
    mark("released_results_replay_advanced");
    equal(&prepared.reference, &prepared.engine.outcome());
    check_old(&points, &old, &prepared.reference);
    mark("released_results_replay_checked");
    let Prepared {
        engine, reference, ..
    } = prepared;
    drop(engine);
    mark("engine_released");
    points.clear();
    drop(reference);
    mark("all_engine_owners_released");
    Ok(())
}

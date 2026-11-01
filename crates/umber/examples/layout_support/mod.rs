//! Native measurement adapter; no changes to interpreter execution.
use std::{hint::black_box, time::Instant};
use umber::{Engine, Outcome, Placement, Progress};

fn complete(engine: &mut Engine<'_>) {
    assert_eq!(engine.advance(usize::MAX).unwrap(), Progress::Complete);
}

fn equal(a: &Outcome, b: &Outcome) {
    assert_eq!(a.counts, b.counts);
    assert_eq!(a.messages, b.messages);
    assert_eq!(a.pages.len(), b.pages.len());
    for i in 0..a.pages.len() {
        let a = a.pages.get(i).unwrap();
        let b = b.pages.get(i).unwrap();
        assert_eq!(a.counts, b.counts);
        assert_eq!(a.output_penalty, b.output_penalty);
        assert_eq!(a.boxes(), b.boxes());
        assert!(a.placements().eq(b.placements()));
        assert_eq!(a.fonts().len(), b.fonts().len());
        for (a, b) in a.fonts().iter().zip(b.fonts()) {
            assert_eq!(a.name(), b.name());
            assert_eq!(a.size(), b.size());
            assert_eq!(a.tfm_bytes(), b.tfm_bytes());
        }
    }
}

fn inspect(outcome: &Outcome) {
    println!("{{\"paragraphs\":{},\"pages\":[", outcome.counts[0]);
    for (i, page) in outcome.pages.iter().enumerate() {
        if i != 0 {
            println!(",");
        }
        print!("{{\"boxes\":{},\"placements\":[", page.boxes().len());
        for (j, placement) in page.placements().enumerate() {
            if j != 0 {
                print!(",");
            }
            match placement {
                Placement::Glyph {
                    font,
                    character,
                    x,
                    y,
                } => {
                    let width = page.fonts()[font as usize]
                        .metrics(character)
                        .unwrap()
                        .width;
                    print!("[{character},{x},{y},{width}]");
                }
                Placement::Rule { .. } => panic!("prose workload contains no rules"),
            }
        }
        print!("]}}");
    }
    println!("]}}");
}

pub fn run(mut mark: impl FnMut(&'static str)) {
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("MODE SOURCE FONT_NAME TFM_PATH");
    let source = std::fs::read(args.next().unwrap()).unwrap();
    let name = args.next().unwrap();
    let tfm = std::fs::read(args.next().unwrap()).unwrap();
    let mut records = Vec::with_capacity(15);
    mark("begin");
    let make = || {
        let mut e = Engine::new(&source);
        e.register_tfm(name.as_bytes(), &tfm).unwrap();
        e
    };
    let mut engine = make();
    if mode == "inspect" {
        complete(&mut engine);
        inspect(&engine.outcome());
        return;
    }
    if mode == "alloc-full" {
        complete(&mut engine);
        mark("complete");
        drop(engine);
        mark("all_engine_owners_released");
        return;
    }
    let mut checkpoints = Vec::new();
    let mut prefixes = Vec::new();
    let mut previous = 0;
    while engine.advance(1).unwrap() != Progress::Complete {
        let n = engine.outcome().counts[0];
        if n > previous && n % 10 == 0 {
            checkpoints.push((n, engine.checkpoint().unwrap()));
            prefixes.push(engine.outcome());
            previous = n;
        }
    }
    let uninterrupted = engine.outcome();
    mark("completed_with_checkpoints_and_prefixes");
    let total = uninterrupted.counts[0];
    let mut fresh = make();
    complete(&mut fresh);
    equal(&uninterrupted, &fresh.outcome());
    drop(fresh);
    mark("fresh_comparison_released");

    if mode == "time" {
        // A warm replay precedes the five retained measured results per position.
        for target in [total / 10, total / 2, total * 9 / 10] {
            let (_, saved) = checkpoints.iter().find(|(n, _)| *n >= target).unwrap();
            let mut old_outputs = Vec::with_capacity(6);
            for repeat in 0..6 {
                let start = Instant::now();
                engine.restore(saved).unwrap();
                let changed_restore = start.elapsed().as_nanos();
                let start = Instant::now();
                for _ in 0..200 {
                    drop(black_box(engine.checkpoint().unwrap()));
                }
                let capture = start.elapsed().as_nanos() as f64 / 200.0;
                let start = Instant::now();
                for _ in 0..200 {
                    engine.restore(black_box(saved)).unwrap();
                }
                let restore = start.elapsed().as_nanos() as f64 / 200.0;
                let start = Instant::now();
                complete(&mut engine);
                let output = engine.outcome();
                let suffix = start.elapsed().as_secs_f64();
                equal(&uninterrupted, &output);
                old_outputs.push(output);
                if repeat > 0 {
                    records.push((target, capture, restore, changed_restore, suffix));
                }
            }
        }
    } else {
        assert_eq!(mode, "alloc");
        let (_, saved) = checkpoints
            .iter()
            .find(|(n, _)| *n >= total * 9 / 10)
            .unwrap();
        engine.restore(saved).unwrap();
        mark("late_checkpoint_restored");
        for _ in 0..200 {
            drop(black_box(engine.checkpoint().unwrap()));
        }
        mark("capture_release_200");
        for _ in 0..200 {
            engine.restore(saved).unwrap();
        }
        mark("restore_200");
        // Fixed live working set: release each regenerated suffix result.
        for (iterations, label) in [(1, "replay_1"), (5, "replay_5"), (20, "replay_20")] {
            for _ in 0..iterations {
                engine.restore(saved).unwrap();
                complete(&mut engine);
                let output = engine.outcome();
                equal(&uninterrupted, &output);
            }
            mark(label);
        }
        // Growing live output: each replay retains a distinct suffix branch.
        let mut branches = Vec::with_capacity(5);
        for _ in 0..5 {
            engine.restore(saved).unwrap();
            complete(&mut engine);
            branches.push(engine.outcome());
        }
        mark("five_suffix_outputs_retained");
        drop(branches);
        mark("five_suffix_outputs_released");
    }
    // Retained prefix endpoints must still describe the same old pages.
    for prefix in &prefixes {
        for (old, final_page) in prefix.pages.iter().zip(uninterrupted.pages.iter()) {
            assert!(old.placements().eq(final_page.placements()));
        }
    }
    drop(engine);
    mark("engine_released");
    drop(checkpoints);
    drop(prefixes);
    drop(uninterrupted);
    mark("all_engine_owners_released");
    if mode == "time" {
        println!("{{\"paragraphs\":{total},\"cadence\":10,\"samples\":[");
        for (i, (target, capture, restore, changed_restore, suffix)) in records.iter().enumerate() {
            if i != 0 {
                println!(",");
            }
            print!(
                "{{\"paragraph\":{target},\"capture_release_ns\":{capture},\"stable_restore_ns\":{restore},\"changed_restore_ns\":{changed_restore},\"suffix_seconds\":{suffix}}}"
            );
        }
        println!("]}}");
    }
}

use super::{Attempt, Buffer, Fact, Issue, Progress, Quality, Side, Stage, Window};

fn quality() -> Quality {
    Quality::new(Window { from: 50, to: 200 }, None).unwrap()
}

fn fact(id: &str, group: &str, side: Side) -> Fact {
    Fact {
        at: 100,
        id: id.into(),
        group: group.into(),
        round: Some("turn".into()),
        call: Some("call".into()),
        side: Some(side),
        class: Some("unclassified".into()),
        event: if side == Side::Before {
            "before".into()
        } else {
            "after".into()
        },
    }
}

fn attempt(id: &str, atom: &str) -> Attempt {
    Attempt {
        schema: "locus.adapter.attempt/v1".into(),
        at: Some(100),
        id: Some(id.into()),
        event: Some("before".into()),
        group: Some("group".into()),
        atom: Some(atom.into()),
        outcome: Progress::Accepted {
            buffer: Buffer::Written,
        },
        issues: Vec::new(),
    }
}

#[test]
fn reconciliation() {
    let mut quality = quality();
    let before = fact("one", "group", Side::Before);
    quality.observe(before.clone()).unwrap();
    quality.observe(before).unwrap();
    quality.observe(fact("two", "group", Side::After)).unwrap();
    let accepted = attempt("attempt", "one");
    quality.attempt(accepted.clone()).unwrap();
    quality.attempt(accepted).unwrap();
    quality.attempt(attempt("pending", "three")).unwrap();
    let report = quality.finish(true, 0, Some(1));
    assert_eq!(report.tools.matched, 1);
    let outcomes = report.outcomes.unwrap();
    assert_eq!(outcomes.observed, 2);
    assert_eq!(outcomes.duplicate, 1);
    let readback = report.readback.unwrap();
    assert_eq!(readback.matched, 1);
    assert_eq!(readback.pending, 1);
    assert_eq!(readback.unexplained, 1);
    assert_eq!(readback.duplicate, 1);
}

#[test]
fn ambiguity() {
    let mut quality = quality();
    quality.observe(fact("one", "group", Side::Before)).unwrap();
    quality.observe(fact("two", "group", Side::Before)).unwrap();
    quality
        .observe(fact("three", "group", Side::After))
        .unwrap();
    quality.observe(fact("four", "other", Side::After)).unwrap();
    let report = quality.finish(false, 0, None);
    assert_eq!(report.tools.matched, 0);
    assert_eq!(report.tools.ambiguous, 1);
    assert_eq!(report.tools.starts, 1);
    assert_eq!(report.tools.closing, 1);
    assert!(report.outcomes.is_none());
    assert!(report.scope.expected.is_none());
}

#[test]
fn rejected() {
    let mut quality = quality();
    let mut rejected = attempt("rejected", "unused");
    rejected.atom = None;
    rejected.outcome = Progress::Rejected {
        stage: Stage::Normalize,
    };
    rejected.issues.push(Issue {
        stage: Stage::Normalize,
        code: "field.invalid".into(),
        field: None,
    });
    quality.attempt(rejected).unwrap();
    let mut failed = attempt("failed", "accepted");
    failed.outcome = Progress::Accepted {
        buffer: Buffer::Failed,
    };
    quality.attempt(failed).unwrap();
    let report = quality.finish(true, 3, None);
    let outcomes = report.outcomes.unwrap();
    assert_eq!(outcomes.rejected, 1);
    assert_eq!(outcomes.accepted, 1);
    assert_eq!(outcomes.normalized, 1);
    assert_eq!(outcomes.failed, 1);
    assert_eq!(outcomes.buffered, 0);
    assert_eq!(report.legacy, 3);
}

#[test]
fn conflict() {
    let mut quality = quality();
    quality.attempt(attempt("same", "one")).unwrap();
    assert!(quality.attempt(attempt("same", "two")).is_err());
    assert!(quality.attempt(attempt("different", "one")).is_err());
    quality
        .observe(fact("same", "group", Side::Before))
        .unwrap();
    assert!(
        quality
            .observe(fact("same", "other", Side::Before))
            .is_err()
    );
}

#[test]
fn scope() {
    let mut quality = Quality::new(
        Window { from: 50, to: 200 },
        Some(["group".into(), "missing".into()].into()),
    )
    .unwrap();
    quality.observe(fact("one", "group", Side::Before)).unwrap();
    let mut outside = attempt("outside", "two");
    outside.at = Some(200);
    quality.attempt(outside).unwrap();
    let mut unscoped = attempt("unscoped", "three");
    unscoped.at = None;
    quality.attempt(unscoped).unwrap();
    let report = quality.finish(true, 0, None);
    assert_eq!(report.scope.expected, Some(2));
    assert_eq!(report.scope.matched, Some(1));
    assert_eq!(report.outcomes.unwrap().observed, 0);
    assert_eq!(report.unscoped, 1);
}

#[test]
fn invalid() {
    let mut bad = attempt("attempt", "atom");
    bad.outcome = Progress::Rejected {
        stage: Stage::Buffer,
    };
    assert!(bad.validate().is_err());
    bad = attempt("attempt", "atom");
    bad.schema = "unknown".into();
    assert!(bad.validate().is_err());
    let mut quality = quality();
    assert!(
        quality
            .observe(fact("one", &"x".repeat(513), Side::Before))
            .is_err()
    );
}

#[test]
fn identity() {
    let mut quality = quality();
    let mut unlinked = attempt("attempt", "unused");
    unlinked.atom = None;
    unlinked.outcome = Progress::Accepted {
        buffer: Buffer::Unknown,
    };
    quality.attempt(unlinked).unwrap();
    let report = quality.finish(true, 0, None);
    assert_eq!(report.outcomes.unwrap().unlinked, 1);
    assert!(
        report
            .unknowns
            .contains(&"accepted-atom-identity-unavailable")
    );
}

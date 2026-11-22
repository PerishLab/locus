use super::{Kind, Metric, Query};
use serde_json::{Value, json};

fn query(kind: Kind, step: u64) -> Metric {
    Metric::new(Query {
        name: "load".into(),
        unit: "count".into(),
        producer: "node".into(),
        from: 0,
        to: 135,
        step,
        kind,
        group: None,
    })
    .unwrap()
}

fn atom(id: &str, at: u64, value: u64, epoch: &str) -> Value {
    json!({"id":id,"at":at,"producer":"node","choices":[],"context":{
        "metric.name":"load","metric.series":"pod-1","metric.kind":"counter",
        "metric.unit":"count","metric.epoch":epoch},"payload":{"value":value}})
}

fn feed(metric: &mut Metric, sample: &Value) {
    metric
        .observe(&serde_json::to_vec(sample).unwrap())
        .unwrap();
}

#[test]
fn counter() {
    let mut metric = query(Kind::Counter, 135);
    for (at, value) in [
        (0, 0),
        (15, 3),
        (45, 9),
        (30, 8),
        (60, 1),
        (75, 4),
        (90, 4),
        (105, 7),
        (120, 9),
        (135, 88),
    ] {
        let epoch = if at < 60 { "before" } else { "after" };
        let sample = atom(&at.to_string(), at, value, epoch);
        feed(&mut metric, &sample);
        feed(&mut metric, &sample);
    }
    let report = metric.finish();
    let b = &report.groups[0].buckets[0];
    assert_eq!((b.samples, b.evidence.len(), b.value), (9, 9, Some(18.0)));
}

#[test]
fn quality() {
    let mut metric = query(Kind::Gauge, 15);
    for (id, at, value) in [("a", 0, 0), ("b", 0, 0), ("c", 30, 1), ("d", 30, 2)] {
        let mut sample = atom(id, at, value, "");
        sample["context"]["metric.kind"] = json!("gauge");
        feed(&mut metric, &sample);
    }
    let groups = metric.finish().groups;
    let b = &groups[0].buckets;
    assert_eq!(
        (b[0].samples, b[0].evidence.len(), b[0].value),
        (1, 2, Some(0.0))
    );
    assert_eq!((&*b[1].quality, b[1].value), ("missing", None));
    assert_eq!((&*b[2].quality, b[2].value), ("conflict", None));
}

#[test]
fn decrease() {
    let mut metric = query(Kind::Counter, 15);
    feed(&mut metric, &atom("a", 0, 2, ""));
    feed(&mut metric, &atom("b", 15, 1, ""));
    let groups = metric.finish().groups;
    assert_eq!(&groups[0].buckets[0].quality, "baseline");
    assert_eq!(&groups[0].buckets[1].quality, "discontinuous");
    assert!(groups[0].buckets[1].value.is_none());
}

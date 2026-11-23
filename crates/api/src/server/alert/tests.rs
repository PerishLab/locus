use super::ledger::{Delivery, Seat};
use super::rule::{Rules, SECOND};
use super::state::{Phase, Proof, State};
use serde_json::json;

fn rules() -> Rules {
    serde_json::from_value(
        json!({"interval":1,"origin":"http://127.0.0.1:8080/","rules":[{
            "name":"slow","revision":1,"producer":"probe","subject":"evaluator","metric":"duration",
            "unit":"seconds","kind":"gauge","above":0.5,"hold":2,
            "recipients":[{"subject":"reader","endpoint":"http://127.0.0.1:8081/"}]
        }]}),
    )
    .unwrap()
}

fn point(second: u64, value: f64) -> Proof {
    Proof {
        from: second * SECOND,
        to: (second + 1) * SECOND,
        latest: Some(second * SECOND),
        quality: "observed".into(),
        samples: 1,
        value: Some(value),
    }
}

#[test]
fn recovery() {
    let rules = rules();
    let rule = &rules.rules[0];
    let mut state = State::new(rule, rules.interval, rules.delay);
    assert_eq!(state.advance(rule, &point(2, 0.8)), None);
    assert_eq!(state.advance(rule, &point(3, 0.8)), None);
    assert_eq!(state.advance(rule, &point(4, 0.8)), Some(Phase::Firing));
    let missing = Proof::missing(5 * SECOND, 6 * SECOND, "missing");
    assert_eq!(state.advance(rule, &missing), Some(Phase::Unknown));
    state.advance(rule, &point(5, 0.2));
    assert_eq!(state.phase, Phase::Unknown);
    assert_eq!(state.advance(rule, &point(6, 0.2)), Some(Phase::Recovered));
}

#[test]
fn restart() {
    let rules = rules();
    let rule = &rules.rules[0];
    let path = tempfile::tempdir().unwrap();
    let seat = Seat::open(path.path()).unwrap();
    assert!(Seat::open(path.path()).is_err());
    let mut ledger = seat.load().unwrap();
    let mut state = State::new(rule, rules.interval, rules.delay);
    state.advance(rule, &point(1, 0.0));
    ledger.rules.insert(rule.name.clone(), state);
    ledger.notices.extend(super::send::prepare(
        &rules,
        rule,
        Phase::Firing,
        &point(3, 0.8),
    ));
    seat.save(&ledger).unwrap();
    let id = ledger.notices[0].id.clone();
    drop(seat);
    let restarted = Seat::open(path.path()).unwrap().load().unwrap();
    assert_eq!(restarted.notices[0].id, id);
    assert!(restarted.notices[0].delivery == Delivery::Pending);
    assert_eq!(restarted.rules[&rule.name].evaluated, 2 * SECOND);
}

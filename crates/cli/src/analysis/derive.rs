use locus_derivation::evidence::Evidence;
use locus_derivation::{inspect, span};
use std::io::Cursor;

#[locus::trace(with = plumb::observation::view())]
pub fn inspection(
    config: &inspect::Config,
    bytes: &[u8],
    retained: Option<u64>,
) -> Result<(inspect::Report, u64), String> {
    let mut inspection = inspect::Inspection::new(config);
    let records = crate::input::scan(Cursor::new(bytes), |atom, encoded| {
        inspection.observe(&atom, encoded)
    })?;
    Ok((inspection.finish().retained(retained), records))
}

#[locus::trace(with = plumb::observation::view())]
pub fn spans(bytes: &[u8], retained: Option<u64>) -> Result<span::Report, String> {
    let mut span = span::Span::default();
    crate::input::scan(Cursor::new(bytes), |atom, _| span.observe(&atom))?;
    Ok(span.finish().retained(retained))
}

#[locus::trace(with = plumb::observation::view())]
pub fn evidence(inspection: &inspect::Report, bytes: &[u8]) -> Result<Evidence, String> {
    let mut evidence = Evidence::new(inspection.findings())?;
    crate::input::scan(Cursor::new(bytes), |atom, encoded| {
        evidence.observe(&atom, encoded)
    })?;
    Ok(evidence)
}

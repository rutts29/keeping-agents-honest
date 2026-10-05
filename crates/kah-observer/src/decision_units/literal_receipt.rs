//! Exact observed properties are code facts, never interpretive model votes.
use super::*;

pub(super) fn verify(
    r: &Requirement,
    citations: &[Value],
    documents: &[Document],
) -> Result<(), String> {
    let Thinking::AtomicFact { fact, .. } = &r.judgment else {
        return Ok(());
    };
    let check = &r.checks[fact.check()];
    let index = if matches!(fact, Literal::Sha { .. }) {
        check.sha_citation
    } else {
        check.result_citation
    };
    let citation = &citations[index];
    let doc = documents
        .iter()
        .find(|d| d.id == citation["source"])
        .ok_or("REVIEW_LITERAL_SOURCE stage=receipt")?;
    let records = citation["native_records"]
        .as_array()
        .ok_or("REVIEW_LITERAL_UNOBSERVED stage=receipt")?;
    let matches = match fact {
        Literal::ExitStatus { expected, .. } => {
            let values = records
                .iter()
                .filter_map(|r| r["exit_code"].as_i64())
                .collect::<Vec<_>>();
            if values.is_empty() {
                return Err(
                    "REVIEW_LITERAL_UNOBSERVED stage=receipt: no native exit status".into(),
                );
            }
            values.iter().all(|v| v == expected)
        }
        Literal::Sha { expected, .. } => {
            let lines = lines(citation, records, doc)?;
            let hashes = lines
                .iter()
                .map(|s| s.trim())
                .filter(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()))
                .collect::<Vec<_>>();
            !hashes.is_empty() && hashes.iter().all(|s| *s == expected)
        }
        Literal::OutputLine { expected, .. } => {
            lines(citation, records, doc)?.iter().any(|s| s == expected)
        }
    };
    if !matches {
        return Err(format!(
            "REVIEW_LITERAL_CONTRADICTS stage=receipt: {} cited property differs from its expected value",
            r.id
        ));
    }
    Ok(())
}

fn lines(citation: &Value, records: &[Value], doc: &Document) -> Result<Vec<String>, String> {
    let start = citation["start_byte"]
        .as_u64()
        .ok_or("REVIEW_LITERAL_RANGE stage=receipt")? as usize;
    let end = citation["end_byte"]
        .as_u64()
        .ok_or("REVIEW_LITERAL_RANGE stage=receipt")? as usize;
    let mut result = vec![];
    for range in records
        .iter()
        .flat_map(|r| r["output_ranges"].as_array().into_iter().flatten())
    {
        let a = range[0]
            .as_u64()
            .ok_or("REVIEW_LITERAL_RANGE stage=receipt")? as usize;
        let b = range[1]
            .as_u64()
            .ok_or("REVIEW_LITERAL_RANGE stage=receipt")? as usize;
        let text = doc
            .text
            .get(a..b)
            .ok_or("REVIEW_LITERAL_RANGE stage=receipt")?;
        let mut offset = a;
        for line in text.split_inclusive('\n') {
            let value = line
                .strip_suffix('\n')
                .unwrap_or(line)
                .strip_suffix('\r')
                .unwrap_or(line.strip_suffix('\n').unwrap_or(line));
            if offset >= start && offset + value.len() <= end {
                result.push(value.into());
            }
            offset += line.len();
        }
    }
    if result.is_empty() {
        return Err(
            "REVIEW_LITERAL_UNOBSERVED stage=receipt: cite a complete native output line".into(),
        );
    }
    Ok(result)
}

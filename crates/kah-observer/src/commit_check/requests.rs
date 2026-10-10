use super::*;
pub(super) fn literal(input: &Input, touched: &BTreeSet<String>) -> Result<Prepared, String> {
    let mut checks = BTreeMap::new();
    let mut questions = BTreeMap::new();
    for id in touched {
        let rule = &input.rules[id];
        if !rule.literal_check {
            continue;
        }
        let authority = &input.intent.items[&rule.authority];
        let hunks = rule
            .hunks
            .iter()
            .map(|id| {
                input
                    .hunks
                    .get(id)
                    .map(|h| (id.clone(), h.clone()))
                    .ok_or_else(|| format!("MEASURE_HUNK_MISSING stage=literal: {id}"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        if hunks.is_empty() {
            continue;
        }
        let name = key(id);
        checks.insert(name.clone(), json!({"requirement":id,"rule":authority.text,"rule_origin":authority.origin,
            "source_ids":authority.sources,"diff_hunks":hunks,"current_permissions":input.intent.permissions_for(&rule.authority)?}));
        questions.insert(format!("rule.{name}"), Query::Noul { instructions: format!("Does the added or changed code in `checks.{name}.diff_hunks` do what `checks.{name}.rule` forbids or contradict that rule? Unfinished or not-yet-implemented work is not a violation. A change explicitly authorized by `checks.{name}.current_permissions` is not a violation.") });
    }
    Ok(Prepared {
        state: json!({"checks":checks}),
        questions,
    })
}
type CitationPlan = (Prepared, BTreeSet<String>, Vec<Value>);
pub(super) fn citations(input: &Input, touched: &BTreeSet<String>) -> Result<CitationPlan, String> {
    let mut rows = BTreeMap::new();
    let mut questions = BTreeMap::new();
    let mut absence = BTreeSet::new();
    let mut signals = vec![];
    let mut seen = BTreeSet::new();
    for row in &input.matrix {
        if !touched.contains(&row.requirement) || !seen.insert(&row.requirement) {
            return Err(format!(
                "MEASURE_MATRIX_SCOPE stage=matrix: {}",
                row.requirement
            ));
        }
        if absence_row(input, row)? {
            absence.insert(row.requirement.clone());
            continue;
        }
        if row.status == Status::Unclear && row.quotes.is_empty() {
            continue;
        }
        if row.quotes.is_empty() {
            signals.push(json!({"requirement":row.requirement,"error":"MEASURE_QUOTE_MISSING","action":"native_confirmation"}));
            continue;
        }
        let quotes = verified_quotes(input, row, &mut signals);
        if row.status == Status::Unclear {
            continue;
        }
        if quotes.len() != row.quotes.len() {
            continue;
        }
        let name = key(&row.requirement);
        rows.insert(
            name.clone(),
            json!({"requirement":input.intent.items[&input.rules[&row.requirement].authority].text,
            "finding":row.finding,"status":row.status,"quoted_code":quotes}),
        );
        questions.insert(
            format!("citation.{name}"),
            Query::Choice {
                instructions: format!(
                    "How does `rows.{name}.quoted_code` relate to `rows.{name}.finding`?"
                ),
                criteria: citation_options(),
            },
        );
    }
    Ok((
        Prepared {
            state: json!({"rows":rows}),
            questions,
        },
        absence,
        signals,
    ))
}

fn citation_options() -> Vec<(String, String)> {
    vec![
        (
            "says_nothing".into(),
            "The quoted code does not establish the finding or its opposite.".into(),
        ),
        (
            "contradicts".into(),
            "The quoted code states or directly implements the opposite of the finding.".into(),
        ),
        (
            "supports".into(),
            "The quoted code states or directly implements the finding.".into(),
        ),
    ]
}

fn verified_quotes(
    input: &Input,
    row: &Row,
    signals: &mut Vec<Value>,
) -> BTreeMap<String, CodeQuote> {
    let mut quotes = BTreeMap::new();
    for (index, quote) in row.quotes.iter().enumerate() {
        if let Err(error) = verify_quote(input, quote) {
            signals.push(
                json!({"requirement":row.requirement,"error":error,"action":if row.status==Status::Unclear {"checkpoint_unclear"} else {"native_confirmation"}}),
            );
        } else {
            quotes.insert(key(&format!("{}:{index}", quote.file)), quote.clone());
        }
    }
    quotes
}

fn absence_row(input: &Input, row: &Row) -> Result<bool, String> {
    if row.kind != FindingKind::NotViolated {
        return Ok(false);
    }
    let rule = &input.rules[&row.requirement];
    if !rule.prohibition || !rule.literal_check || row.status != Status::Satisfied {
        return Err(format!(
            "MEASURE_MATRIX_ABSENCE stage=matrix: {}",
            row.requirement
        ));
    }
    Ok(true)
}
